use super::super::check_program_with_publication_inspector;
use crate::binder::declaration::{DeclId, TypeFragmentKind};
use crate::check::checker::context::{CheckerEffects, DeclTypes, HeaderFragmentBinding, TypeDecl};
use crate::check::checker::reporting_record::CheckerRecord;
use crate::check::checker::type_groups::{
    InterfaceAlternativeKind, PublishedTypeGroupSurface, PublishedTypeGroupTerminal,
    PublishedTypeParameterDefault,
};
use crate::check::query::{SemanticQueryCoordinator, SemanticQueryState};
use crate::class_semantics::{DemandOutcome, Exhaustion};
use crate::diagnostics::DiagnosticCode;
use crate::driver::{check_project, check_source, FileInput};
use crate::source::ModuleOrdinal;
use crate::span::Span;
use crate::types::repr::{ClassId, LiteralValue, TypeParamId, TypeTag};
use crate::types::store::{Store, TypeId};
use crate::types::Interner;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use rustc_hash::FxHashSet;

#[derive(Debug)]
struct ReservedClassInterfaceHeaders {
    bound_fragments: Vec<(DeclId, TypeFragmentKind)>,
    class_declaration: DeclId,
    class_id: ClassId,
    class_params: Vec<TypeParamId>,
    headers: Vec<HeaderFragmentBinding>,
    next_type_param: u32,
    next_class_id: u32,
}

fn reserve_class_interface_headers(source: &str) -> ReservedClassInterfaceHeaders {
    let prelude_allocator = Allocator::default();
    let user_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
    let user = Parser::new(&user_allocator, source, SourceType::ts()).parse();
    assert!(user.diagnostics.is_empty(), "{:?}", user.diagnostics);
    let binder = crate::binder::bind_module_with_prelude(&prelude.program, &user.program);
    let group = binder
        .graph
        .get(binder.module)
        .and_then(|scope| scope.lookup_local("Mixed"))
        .and_then(|symbol| binder.symbols.get(symbol))
        .and_then(|symbol| symbol.ty)
        .expect("Mixed type group");
    let bound_fragments = binder
        .type_groups
        .get(group)
        .expect("Mixed group metadata")
        .fragments
        .iter()
        .map(|fragment| (fragment.declaration, fragment.kind))
        .collect();
    let mut interner = Interner::with_intrinsics();
    let mut declarations = Vec::new();
    let mut resolved = vec![None; binder.type_groups.len()];
    let mut next_type_param = 0;
    let mut next_class_id = 0;
    crate::check::checker::decls::reserve_type_decls(
        &mut interner,
        &binder,
        binder.module,
        &user.program,
        &mut next_type_param,
        &mut next_class_id,
        &mut declarations,
        &mut resolved,
    );
    let TypeDecl::Class {
        declaration,
        class_id,
        class_params,
        header_fragments,
        interfaces,
        ..
    } = declarations.get(group.index()).expect("Mixed reservation")
    else {
        panic!("Mixed is retained by its class-owned draft")
    };
    assert!(!interfaces.is_empty(), "mixed draft retains interface AST");
    ReservedClassInterfaceHeaders {
        bound_fragments,
        class_declaration: *declaration,
        class_id: *class_id,
        class_params: class_params.clone(),
        headers: header_fragments.clone(),
        next_type_param,
        next_class_id,
    }
}

#[test]
fn interface_relation_exhaustion_stays_with_its_lexical_owner_without_failure_diagnostic() {
    let prelude_allocator = Allocator::default();
    let user_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
    let user = Parser::new(&user_allocator, "", SourceType::ts()).parse();
    let binder = crate::binder::bind_module_with_prelude(&prelude.program, &user.program);
    let mut interner = Interner::with_intrinsics();
    let mut pass = super::super::build_pass(
        &mut interner,
        &binder,
        Vec::new(),
        vec![None; binder.type_groups.len()],
        DeclTypes::new(binder.decl_count),
        0,
    );
    pass.type_environment = crate::check::checker::type_groups::TypeEnvironmentState::Published(
        crate::check::checker::type_groups::PublishedTypeEnvironment::empty(),
    );
    let unrelated = pass.event_store.reserve_event(ModuleOrdinal::new(0), 10);
    let owner = pass.event_store.reserve_event(ModuleOrdinal::new(0), 20);
    let effects = CheckerEffects::new(owner.primary);
    let span = Span::new(21, 22);

    let (effects, failed) = super::super::consume_interface_relation_decision(
        &mut pass,
        effects,
        Err(Exhaustion::ClassProjectionBudget),
        span,
    );

    assert_eq!(failed, None, "exhaustion is not a failed relation");
    assert_eq!(effects.records.owner(), owner.primary);
    assert_eq!(effects.records.len(), 1);
    pass.event_store
        .complete(unrelated.primary, Vec::new())
        .expect("unrelated owner completes independently");
    pass.event_store
        .commit(effects.records)
        .expect("relation owner commits its recovery record");
    let records = std::mem::take(&mut pass.event_store)
        .finish()
        .expect("all test owners completed");
    assert_eq!(records.len(), 1);
    let CheckerRecord::Incomplete(record) = &records[0].1 else {
        panic!("relation exhaustion must not become a failure diagnostic")
    };
    assert_eq!(record.id, "relation/class-projection-budget");
    assert_eq!(record.span, span);
}

fn published_group<'a>(
    binder: &crate::binder::Binder,
    environment: &'a crate::check::checker::type_groups::PublishedTypeEnvironment,
    name: &str,
) -> &'a crate::check::checker::type_groups::PublishedTypeGroup {
    let group = binder
        .graph
        .get(binder.module)
        .and_then(|scope| scope.lookup_local(name))
        .and_then(|symbol| binder.symbols.get(symbol))
        .and_then(|symbol| symbol.ty)
        .expect("named type group");
    let Some(PublishedTypeGroupTerminal::Ready(group)) = environment.groups().get(group) else {
        panic!("{name} must publish a ready type group")
    };
    group
}

fn free_type_params(store: &Store, root: TypeId) -> FxHashSet<TypeParamId> {
    fn visit(
        store: &Store,
        ty: TypeId,
        bound: &FxHashSet<TypeParamId>,
        free: &mut FxHashSet<TypeParamId>,
        seen: &mut FxHashSet<(TypeId, Vec<TypeParamId>)>,
    ) {
        let mut bound_key = bound.iter().copied().collect::<Vec<_>>();
        bound_key.sort_unstable();
        if !seen.insert((ty, bound_key)) {
            return;
        }
        match store.tag(ty) {
            TypeTag::TypeParam => {
                let parameter = store.type_param(ty).expect("type parameter payload").id;
                if !bound.contains(&parameter) {
                    free.insert(parameter);
                }
            }
            TypeTag::Object => {
                let object = store.object_type(ty).expect("object payload");
                for property in &object.properties {
                    visit(store, property.ty, bound, free, seen);
                    if let Some(write_ty) = property.write_ty {
                        visit(store, write_ty, bound, free, seen);
                    }
                }
                for child in object
                    .string_index
                    .into_iter()
                    .chain(object.number_index)
                    .chain(object.call_signatures.iter().copied())
                    .chain(object.construct_signatures.iter().copied())
                {
                    visit(store, child, bound, free, seen);
                }
            }
            TypeTag::Function => {
                let function = store.function_type(ty).expect("function payload");
                let mut function_bound = bound.clone();
                function_bound.extend(function.type_params.iter().map(|parameter| parameter.id));
                for parameter in &function.type_params {
                    for child in [parameter.constraint, parameter.default]
                        .into_iter()
                        .flatten()
                    {
                        visit(store, child, &function_bound, free, seen);
                    }
                }
                if let Some(receiver) = function.receiver {
                    visit(store, receiver, &function_bound, free, seen);
                }
                for parameter in &function.params {
                    visit(store, parameter.ty, &function_bound, free, seen);
                }
                visit(store, function.ret, &function_bound, free, seen);
            }
            TypeTag::Union => {
                for child in store.union_members(ty).expect("union payload") {
                    visit(store, *child, bound, free, seen);
                }
            }
            TypeTag::Intersection => {
                for child in store
                    .intersection_members(ty)
                    .expect("intersection payload")
                {
                    visit(store, *child, bound, free, seen);
                }
            }
            TypeTag::Array => visit(
                store,
                store.array_type(ty).expect("array payload").element,
                bound,
                free,
                seen,
            ),
            TypeTag::Tuple => {
                let tuple = store.tuple_type(ty).expect("tuple payload");
                for child in &tuple.elements {
                    visit(store, *child, bound, free, seen);
                }
                if let Some(rest) = tuple.rest {
                    visit(store, rest.ty, bound, free, seen);
                }
            }
            TypeTag::Readonly => visit(
                store,
                store.readonly_operand(ty).expect("readonly payload"),
                bound,
                free,
                seen,
            ),
            TypeTag::Conditional => {
                let conditional = store.conditional_type(ty).expect("conditional payload");
                for child in [
                    conditional.check,
                    conditional.extends_ty,
                    conditional.true_branch,
                    conditional.false_branch,
                ] {
                    visit(store, child, bound, free, seen);
                }
            }
            TypeTag::Instantiation => {
                let instantiation = store.instantiation_type(ty).expect("instantiation payload");
                let mut base_bound = bound.clone();
                base_bound.extend(instantiation.args.iter().map(|(parameter, _)| *parameter));
                visit(store, instantiation.base, &base_bound, free, seen);
                for (_, argument) in &instantiation.args {
                    visit(store, *argument, bound, free, seen);
                }
            }
            TypeTag::ClassInstance => {
                let instance = store
                    .class_instance_type(ty)
                    .expect("class application payload");
                for argument in &instance.args {
                    visit(store, *argument, bound, free, seen);
                }
            }
            TypeTag::Mapped => {
                let mapped = store.mapped_type(ty).expect("mapped payload");
                for child in [
                    Some(mapped.key_source),
                    Some(mapped.value_template),
                    mapped.modifiers_source,
                ]
                .into_iter()
                .flatten()
                {
                    visit(store, child, bound, free, seen);
                }
            }
            TypeTag::Template => {
                let template = store.template_type(ty).expect("template payload");
                for hole in &template.holes {
                    visit(store, *hole, bound, free, seen);
                }
            }
            TypeTag::Keyof => visit(
                store,
                store.keyof_operand(ty).expect("keyof payload"),
                bound,
                free,
                seen,
            ),
            TypeTag::DeferredIndexedAccess => {
                let access = store
                    .deferred_indexed_access_type(ty)
                    .expect("indexed access payload");
                visit(store, access.object, bound, free, seen);
                visit(store, access.index, bound, free, seen);
            }
            TypeTag::Intrinsic | TypeTag::Literal | TypeTag::Infer | TypeTag::MappedValue => {}
        }
    }

    let mut free = FxHashSet::default();
    visit(
        store,
        root,
        &FxHashSet::default(),
        &mut free,
        &mut FxHashSet::default(),
    );
    free
}

fn assert_published_types_are_owner_closed(
    store: &Store,
    group: &crate::check::checker::type_groups::PublishedTypeGroup,
) {
    let parameters = group.parameters.iter().copied().collect::<FxHashSet<_>>();
    let assert_closed = |ty| {
        let free = free_type_params(store, ty);
        assert!(
            free.is_subset(&parameters),
            "published group {} type {ty:?} contains foreign free parameters: {free:?}; owners: {parameters:?}",
            group.name
        );
    };
    if let PublishedTypeGroupSurface::Template(root) = group.surface {
        assert_closed(root);
    }
    for default in &group.parameter_defaults {
        if let PublishedTypeParameterDefault::Ready(default) = default {
            assert_closed(*default);
        }
    }
    for alternative in &group.conflict_alternatives {
        for ty in &alternative.types {
            assert_closed(*ty);
        }
    }
}

#[test]
fn heritage_property_metadata_is_decided_without_mutating_published_surfaces() {
    let source = "\
interface RequiredBase { value: string | undefined }
interface OptionalDerived extends RequiredBase { value?: string }
interface OptionalBase { value?: string }
interface RequiredDerived extends OptionalBase { value: string | undefined }
interface ReadonlyBase { readonly value: string }
interface MutableDerived extends ReadonlyBase { value: string }
interface MutableBase { value: string }
interface ReadonlyDerived extends MutableBase { readonly value: string }
interface OptionalLeft { metadata?: string }
interface RequiredRight { metadata: string | undefined }
interface OptionalConflict extends OptionalLeft, RequiredRight {}
interface ReadonlyLeft { readonly mode: string }
interface WritableRight { mode: string }
interface ReadonlyConflict extends ReadonlyLeft, WritableRight {}
";
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut interner = Interner::with_intrinsics();
    let mut captured = None;

    let result = check_program_with_publication_inspector(
        &mut interner,
        &parsed.program,
        |binder, environment, interner| {
            let optional = published_group(binder, environment, "OptionalDerived");
            let PublishedTypeGroupSurface::Template(optional_root) = optional.surface else {
                panic!("OptionalDerived is an interface template")
            };
            let optional_surface = format!(
                "{:?}",
                interner
                    .store()
                    .object_type(optional_root)
                    .expect("OptionalDerived object")
            );
            let optional_conflict = published_group(binder, environment, "OptionalConflict");
            let readonly_conflict = published_group(binder, environment, "ReadonlyConflict");
            assert!(optional_conflict
                .conflict_alternatives
                .iter()
                .any(|alternative| {
                    alternative.kind == InterfaceAlternativeKind::Heritage
                        && alternative.key == "metadata"
                        && alternative.types.len() == 2
                }));
            assert!(readonly_conflict
                .conflict_alternatives
                .iter()
                .any(|alternative| {
                    alternative.kind == InterfaceAlternativeKind::Heritage
                        && alternative.key == "mode"
                        && alternative.types.len() == 2
                }));
            captured = Some((optional_root, optional_surface));
        },
    );

    let (optional_root, optional_surface) = captured.expect("publication inspector ran");
    assert_eq!(
        format!(
            "{:?}",
            interner
                .store()
                .object_type(optional_root)
                .expect("published surface remains addressable")
        ),
        optional_surface
    );
    let heritage_diagnostics = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == DiagnosticCode::TK2430)
        .collect::<Vec<_>>();
    assert_eq!(heritage_diagnostics.len(), 1);
    assert_eq!(
        heritage_diagnostics[0].message,
        "Interface 'OptionalDerived' incorrectly extends interface 'RequiredBase'."
    );
}

#[test]
fn own_index_overlay_reports_bases_in_tsc_order() {
    let source = "\
interface StringBase { [key: string]: string }
interface NumberBase { [key: string]: number }
interface Derived extends StringBase, NumberBase { [key: string]: boolean }
";
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut interner = Interner::with_intrinsics();

    let result =
        check_program_with_publication_inspector(&mut interner, &parsed.program, |_, _, _| {});
    assert_eq!(
        result
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == DiagnosticCode::TK2430)
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>(),
        [
            "Interface 'Derived' incorrectly extends interface 'NumberBase'.",
            "Interface 'Derived' incorrectly extends interface 'StringBase'.",
        ]
    );
}

#[test]
fn method_string_index_conflicts_keep_the_method_source_owner() {
    let source = "\
interface LocalBad {
  [key: string]: number;
  local(value: string): string;
}
interface InheritedIndex { [key: string]: number }
interface InheritedBad extends InheritedIndex {
  inherited(value: string): string;
}
interface LocalGood {
  [key: string]: (value: string) => string;
  local(value: string): string;
}
";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    let lines = crate::span::LineIndex::new(source);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                lines.line_of(diagnostic.span.start),
                diagnostic.code,
                &source[diagnostic.span.range()],
            ))
            .collect::<Vec<_>>(),
        [
            (3, DiagnosticCode::TK2411, "local(value: string): string;"),
            (
                7,
                DiagnosticCode::TK2411,
                "inherited(value: string): string;"
            ),
        ]
    );
}

#[test]
fn complete_own_properties_replace_base_conflicts_with_own_base_validation() {
    let source = "\
interface CompatibleLeft { value: { left: number } }
interface CompatibleRight { value: { right: string } }
interface Compatible extends CompatibleLeft, CompatibleRight { value: { left: number; right: string } }
interface NoOwnLeft { value: { left: number } }
interface NoOwnRight { value: { right: string } }
interface NoOwn extends NoOwnLeft, NoOwnRight {}
interface BadOwnLeft { value: { left: number } }
interface BadOwnRight { value: { right: string } }
interface BadOwn extends BadOwnLeft, BadOwnRight { value: boolean }
declare const queryValue: number;
interface PartialOwnLeft { value: string }
interface PartialOwnRight { value: number }
interface PartialOwn extends PartialOwnLeft, PartialOwnRight { value: typeof queryValue }
";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.message.as_str()))
            .collect::<Vec<_>>(),
        [
            (
                DiagnosticCode::TK2320,
                "Interface cannot simultaneously extend types 'NoOwnLeft' and 'NoOwnRight'.",
            ),
            (
                DiagnosticCode::TK2430,
                "Interface 'BadOwn' incorrectly extends interface 'BadOwnLeft'.",
            ),
            (
                DiagnosticCode::TK2430,
                "Interface 'BadOwn' incorrectly extends interface 'BadOwnRight'.",
            ),
            (
                DiagnosticCode::TK2320,
                "Interface cannot simultaneously extend types 'PartialOwnLeft' and 'PartialOwnRight'.",
            ),
        ]
    );
}

#[test]
fn published_generic_interface_freezes_complete_binder_metadata_and_members() {
    let source = "\
interface Generic<First, Second = string> {
  first: First;
  nested: { second: Second };
}
class Mixed {}
interface Mixed { unavailable: number }
interface Independent<Only = number> { value: Only }
";
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut interner = Interner::with_intrinsics();

    let result = check_program_with_publication_inspector(
        &mut interner,
        &parsed.program,
        |binder, environment, interner| {
            let generic = published_group(binder, environment, "Generic");
            assert_eq!(generic.parameter_names, ["First", "Second"]);
            assert_eq!(generic.parameters.len(), 2);
            assert_eq!(
                generic.parameter_defaults,
                [
                    PublishedTypeParameterDefault::Absent,
                    PublishedTypeParameterDefault::Ready(interner.well_known().string),
                ]
            );
            let PublishedTypeGroupSurface::Template(root) = generic.surface else {
                panic!("Generic is an interface template")
            };
            let object = interner.store().object_type(root).expect("Generic object");
            let first = object.property("first").expect("first property");
            assert_eq!(
                interner
                    .store()
                    .type_param(first.ty)
                    .expect("first binder")
                    .id,
                generic.parameters[0]
            );
            let nested = object.property("nested").expect("nested property");
            let nested_object = interner
                .store()
                .object_type(nested.ty)
                .expect("nested object");
            assert_eq!(
                interner
                    .store()
                    .type_param(
                        nested_object
                            .property("second")
                            .expect("second property")
                            .ty
                    )
                    .expect("second binder")
                    .id,
                generic.parameters[1]
            );
            assert_published_types_are_owner_closed(interner.store(), generic);

            let mixed = binder
                .graph
                .get(binder.module)
                .and_then(|scope| scope.lookup_local("Mixed"))
                .and_then(|symbol| binder.symbols.get(symbol))
                .and_then(|symbol| symbol.ty)
                .expect("mixed group");
            let Some(PublishedTypeGroupTerminal::Ready(mixed)) = environment.groups().get(mixed)
            else {
                panic!("mixed class/interface group is published")
            };
            let PublishedTypeGroupSurface::Class(class) = mixed.surface else {
                panic!("mixed group keeps its class identity")
            };
            let crate::class_semantics::DemandOutcome::Ready(surface) =
                environment.classes().published_class(class)
            else {
                panic!("mixed class surface is ready")
            };
            assert!(interner
                .store()
                .object_type(surface.instance_template())
                .is_some_and(|object| object.property("unavailable").is_some()));
            let independent = published_group(binder, environment, "Independent");
            assert_eq!(independent.parameter_names, ["Only"]);
            assert_eq!(
                independent.parameter_defaults,
                [PublishedTypeParameterDefault::Ready(
                    interner.well_known().number
                )]
            );
            assert_published_types_are_owner_closed(interner.store(), independent);
        },
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn published_dependent_defaults_keep_raw_owner_closed_binders() {
    let source = "\
interface DependentInterface<T = string, U = T> { value: U }
type DependentAlias<T = string, U = T> = { value: U };
interface SelfInterface<T = T> {}
type SelfAlias<T = T> = T;
";
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut interner = Interner::with_intrinsics();

    let result = check_program_with_publication_inspector(
        &mut interner,
        &parsed.program,
        |binder, environment, interner| {
            for name in ["DependentInterface", "DependentAlias"] {
                let group = published_group(binder, environment, name);
                assert_eq!(group.parameters.len(), 2);
                assert_eq!(
                    group.parameter_defaults[0],
                    PublishedTypeParameterDefault::Ready(interner.well_known().string)
                );
                let PublishedTypeParameterDefault::Ready(second_default) =
                    group.parameter_defaults[1]
                else {
                    panic!("{name} keeps its dependent default")
                };
                assert_eq!(
                    interner
                        .store()
                        .type_param(second_default)
                        .expect("dependent default binder")
                        .id,
                    group.parameters[0]
                );
                assert_published_types_are_owner_closed(interner.store(), group);
            }
            for name in ["SelfInterface", "SelfAlias"] {
                let group = published_group(binder, environment, name);
                assert_eq!(
                    group.parameter_defaults,
                    [PublishedTypeParameterDefault::Unsupported]
                );
                assert_published_types_are_owner_closed(interner.store(), group);
            }
        },
    );
    assert_eq!(
        result
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == DiagnosticCode::TK2744)
            .count(),
        2
    );
}

#[test]
fn recursive_member_groups_publish_independently_of_declaration_order() {
    for (source, lazy_group) in [
        (
            "\
interface A<T> { a: T; next: B<T> }
interface B<U> { b: U; next: A<U> }
declare const value: A<number>;
const direct: number = value.next.b;
const multiHop: number = value.next.next.a;
const wrong: string = value.next.b;
",
            "A",
        ),
        (
            "\
interface B<U> { b: U; next: A<U> }
interface A<T> { a: T; next: B<T> }
declare const value: A<number>;
const direct: number = value.next.b;
const multiHop: number = value.next.next.a;
const wrong: string = value.next.b;
",
            "B",
        ),
    ] {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let mut interner = Interner::with_intrinsics();

        let result = check_program_with_publication_inspector(
            &mut interner,
            &parsed.program,
            |binder, environment, interner| {
                for (name, expected) in [("A", ["a", "next"]), ("B", ["b", "next"])] {
                    let group = published_group(binder, environment, name);
                    let PublishedTypeGroupSurface::Template(root) = group.surface else {
                        panic!("{name} is an interface template")
                    };
                    let object = interner
                        .store()
                        .object_type(root)
                        .expect("interface object");
                    assert_eq!(
                        object
                            .properties
                            .iter()
                            .map(|property| property.name.as_str())
                            .collect::<Vec<_>>(),
                        expected
                    );
                    let next = object.property("next").expect("recursive member").ty;
                    if name == lazy_group {
                        assert_eq!(
                            interner.store().tag(next),
                            TypeTag::Instantiation,
                            "{name}.next must preserve its unfinished generic application"
                        );
                        let application = interner
                            .store()
                            .instantiation_type(next)
                            .expect("recursive generic application");
                        assert_eq!(application.args.len(), 1);
                        assert_eq!(
                            interner
                                .store()
                                .type_param(application.args[0].1)
                                .expect("recursive argument remains the owner binder")
                                .id,
                            group.parameters[0]
                        );
                    } else {
                        assert_eq!(
                            interner.store().tag(next),
                            TypeTag::Object,
                            "{name}.next may eagerly project its frozen dependency"
                        );
                    }
                    assert_published_types_are_owner_closed(interner.store(), group);
                }
            },
        );
        assert_eq!(
            result
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            [DiagnosticCode::TK2322]
        );
    }
}

#[test]
fn pending_forward_generic_reference_stays_lazy_until_publication() {
    let source = "\
interface Forward<T> { later: Later<T> }
interface Later<U> { value: U }
interface Self<T> { next: Self<T> }
declare const forward: Forward<number>;
const value: number = forward.later.value;
const wrong: string = forward.later.value;
";
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut interner = Interner::with_intrinsics();

    let result = check_program_with_publication_inspector(
        &mut interner,
        &parsed.program,
        |binder, environment, interner| {
            let forward = published_group(binder, environment, "Forward");
            let PublishedTypeGroupSurface::Template(root) = forward.surface else {
                panic!("Forward is an interface template")
            };
            let later = interner
                .store()
                .object_type(root)
                .and_then(|object| object.property("later"))
                .expect("forward member")
                .ty;
            assert_eq!(interner.store().tag(later), TypeTag::Instantiation);
            let application = interner
                .store()
                .instantiation_type(later)
                .expect("forward generic application");
            assert_eq!(application.args.len(), 1);
            assert_eq!(
                interner
                    .store()
                    .type_param(application.args[0].1)
                    .expect("forward argument remains the owner binder")
                    .id,
                forward.parameters[0]
            );
            assert_published_types_are_owner_closed(interner.store(), forward);

            let recursive = published_group(binder, environment, "Self");
            let PublishedTypeGroupSurface::Template(recursive_root) = recursive.surface else {
                panic!("Self is an interface template")
            };
            let recursive_next = interner
                .store()
                .object_type(recursive_root)
                .and_then(|object| object.property("next"))
                .expect("recursive member")
                .ty;
            assert_eq!(
                interner.store().tag(recursive_next),
                TypeTag::Instantiation,
                "a Building self-reference must remain lazy"
            );
            let recursive_application = interner
                .store()
                .instantiation_type(recursive_next)
                .expect("recursive self application");
            assert_eq!(recursive_application.args.len(), 1);
            assert_eq!(
                interner
                    .store()
                    .type_param(recursive_application.args[0].1)
                    .expect("self argument remains the owner binder")
                    .id,
                recursive.parameters[0]
            );
            assert_published_types_are_owner_closed(interner.store(), recursive);
        },
    );
    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [DiagnosticCode::TK2322]
    );
}

#[test]
fn published_interface_retains_conflicting_typed_alternatives_and_first_surface() {
    let source = "\
interface Merged { value: 101 }
interface Merged { value: 202 }
";
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut interner = Interner::with_intrinsics();

    let result = check_program_with_publication_inspector(
        &mut interner,
        &parsed.program,
        |binder, environment, interner| {
            let group = binder
                .graph
                .get(binder.module)
                .and_then(|scope| scope.lookup_local("Merged"))
                .and_then(|symbol| binder.symbols.get(symbol))
                .and_then(|symbol| symbol.ty)
                .expect("merged interface type group");
            let PublishedTypeGroupTerminal::Ready(published) = environment
                .groups()
                .get(group)
                .expect("merged interface published terminal")
            else {
                panic!("merged interface remains a ready typed endpoint")
            };
            let PublishedTypeGroupSurface::Template(surface) = published.surface else {
                panic!("interface publishes one structural template")
            };
            let property = interner
                .store()
                .object_type(surface)
                .and_then(|object| object.property("value"))
                .expect("first interface member remains observable");
            assert_eq!(
                interner.store().literal_value(property.ty),
                Some(&LiteralValue::Number(101.0))
            );

            let alternative = published
                .conflict_alternatives
                .iter()
                .find(|alternative| {
                    alternative.kind == InterfaceAlternativeKind::Member
                        && alternative.key == "value"
                })
                .expect("conflicting member alternatives are frozen owner-free");
            let values: Vec<_> = alternative
                .types
                .iter()
                .map(|ty| interner.store().literal_value(*ty))
                .collect();
            assert_eq!(
                values,
                vec![
                    Some(&LiteralValue::Number(101.0)),
                    Some(&LiteralValue::Number(202.0)),
                ]
            );
        },
    );
    assert_eq!(result.diagnostics.len(), 1);
}

#[test]
fn class_first_owned_draft_reserves_exact_interface_recovery_headers() {
    let reserved = reserve_class_interface_headers(
        "class Mixed<T, U> {} interface Mixed<T, Renamed, Extra> {} interface Mixed<T, U> {}",
    );

    assert_eq!(reserved.next_class_id, 1);
    assert_eq!(reserved.class_id, ClassId(0));
    assert_eq!(
        reserved
            .headers
            .iter()
            .map(|header| header.declaration)
            .collect::<Vec<_>>(),
        reserved
            .bound_fragments
            .iter()
            .map(|(declaration, _)| *declaration)
            .collect::<Vec<_>>()
    );
    let class_index = reserved
        .bound_fragments
        .iter()
        .position(|(_, kind)| *kind == TypeFragmentKind::Class)
        .expect("sole class fragment");
    assert_eq!(
        reserved.class_declaration,
        reserved.headers[class_index].declaration
    );
    assert_eq!(
        reserved.class_params,
        reserved.headers[class_index]
            .parameters
            .iter()
            .map(|parameter| parameter.id)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        reserved
            .headers
            .iter()
            .map(|header| {
                header
                    .parameters
                    .iter()
                    .map(|parameter| parameter.name.as_str())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>(),
        [
            vec!["T", "U"],
            vec!["T", "Renamed", "Extra"],
            vec!["T", "U"],
        ]
    );
    let class = &reserved.headers[0].parameters;
    let renamed = &reserved.headers[1].parameters;
    let repeated = &reserved.headers[2].parameters;
    assert_eq!(renamed[0].id, class[0].id);
    assert_eq!(repeated[0].id, class[0].id);
    assert_eq!(repeated[1].id, class[1].id);
    assert_ne!(renamed[1].id, class[1].id);
    assert!(
        [class[0].id, class[1].id, renamed[1].id]
            .into_iter()
            .all(|existing| renamed[2].id != existing),
        "the excess Extra slot owns one fresh identity"
    );
    assert_eq!(reserved.next_type_param, 4);
}

#[test]
fn interface_first_owned_draft_preserves_headers_when_reserving_the_class() {
    let reserved = reserve_class_interface_headers(
        "interface Mixed<T, Renamed> {} class Mixed<T, U, V> {} interface Mixed<T, U> {}",
    );

    assert_eq!(reserved.next_class_id, 1);
    assert_eq!(reserved.class_id, ClassId(0));
    assert_eq!(
        reserved
            .headers
            .iter()
            .map(|header| header.declaration)
            .collect::<Vec<_>>(),
        reserved
            .bound_fragments
            .iter()
            .map(|(declaration, _)| *declaration)
            .collect::<Vec<_>>()
    );
    let class_index = reserved
        .bound_fragments
        .iter()
        .position(|(_, kind)| *kind == TypeFragmentKind::Class)
        .expect("sole class fragment");
    assert_eq!(class_index, 1);
    assert_eq!(
        reserved.class_declaration,
        reserved.headers[class_index].declaration
    );
    assert_eq!(
        reserved.class_params,
        reserved.headers[class_index]
            .parameters
            .iter()
            .map(|parameter| parameter.id)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        reserved
            .headers
            .iter()
            .map(|header| {
                header
                    .parameters
                    .iter()
                    .map(|parameter| parameter.name.as_str())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>(),
        [vec!["T", "Renamed"], vec!["T", "U", "V"], vec!["T", "U"],]
    );
    let first = &reserved.headers[0].parameters;
    let class = &reserved.headers[1].parameters;
    let repeated = &reserved.headers[2].parameters;
    assert_eq!(class[0].id, first[0].id);
    assert_ne!(class[1].id, first[1].id);
    assert_eq!(repeated[0].id, class[0].id);
    assert_eq!(repeated[1].id, class[1].id);
    assert!(
        [first[0].id, first[1].id, class[1].id]
            .into_iter()
            .all(|existing| class[2].id != existing),
        "the class-only V slot owns one fresh identity"
    );
    assert_eq!(reserved.next_type_param, 4);
}

#[test]
fn class_interface_merges_publish_the_class_surface_in_both_orders() {
    for source in [
        "class Mixed {} interface Mixed { value: number }",
        "interface Mixed { value: number } class Mixed {}",
    ] {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let mut interner = Interner::with_intrinsics();

        let _ = check_program_with_publication_inspector(
            &mut interner,
            &parsed.program,
            |binder, environment, _| {
                let group = binder
                    .graph
                    .get(binder.module)
                    .and_then(|scope| scope.lookup_local("Mixed"))
                    .and_then(|symbol| binder.symbols.get(symbol))
                    .and_then(|symbol| symbol.ty)
                    .expect("class-interface type group");
                let Some(PublishedTypeGroupTerminal::Ready(published)) =
                    environment.groups().get(group)
                else {
                    panic!("class/interface group publishes one ready class terminal")
                };
                assert!(matches!(
                    published.surface,
                    PublishedTypeGroupSurface::Class(_)
                ));
            },
        );
    }
}

#[test]
fn class_interface_recovery_arguments_keep_distinct_applications_and_projections() {
    let source = "\
class Mixed<T> { own: T }
interface Mixed<U> { added: U }
";
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut interner = Interner::with_intrinsics();
    let mut captured = None;

    let result = check_program_with_publication_inspector(
        &mut interner,
        &parsed.program,
        |binder, environment, interner| {
            let published = published_group(binder, environment, "Mixed");
            let PublishedTypeGroupSurface::Class(class) = published.surface else {
                panic!("Mixed preserves its class endpoint")
            };
            assert_eq!(published.parameters.len(), 2);
            captured = Some((
                class,
                published.parameters.clone(),
                environment.classes().clone(),
                interner.store().len(),
            ));
        },
    );

    let (class, parameters, classes, publication_type_count) =
        captured.expect("publication inspector ran");
    let wk = interner.well_known();
    let left = interner.intern_class_instance(class, vec![wk.string, wk.number]);
    let right = interner.intern_class_instance(class, vec![wk.number, wk.string]);
    assert_ne!(
        left, right,
        "different recovery vectors keep distinct identities"
    );
    assert!(
        left.index() >= publication_type_count && right.index() >= publication_type_count,
        "the test applications are post-publication identities"
    );
    let mut query_state = SemanticQueryState::default();
    let mut next_type_param = parameters
        .iter()
        .map(|parameter| parameter.0)
        .max()
        .map_or(0, |parameter| parameter + 1);
    let mut coordinator = SemanticQueryCoordinator::new(
        &mut interner,
        &classes,
        &mut query_state,
        &mut next_type_param,
    );
    let DemandOutcome::Ready(left_projection) = coordinator.demand(left) else {
        panic!("left recovery application projects")
    };
    let DemandOutcome::Ready(right_projection) = coordinator.demand(right) else {
        panic!("right recovery application projects")
    };
    assert_ne!(
        left_projection, right_projection,
        "different nominal applications project independently"
    );
    let left_object = interner
        .store()
        .object_type(left_projection)
        .expect("left projection is structural");
    assert_eq!(left_object.property("own").unwrap().ty, wk.string);
    assert_eq!(left_object.property("added").unwrap().ty, wk.number);
    let right_object = interner
        .store()
        .object_type(right_projection)
        .expect("right projection is structural");
    assert_eq!(right_object.property("own").unwrap().ty, wk.number);
    assert_eq!(right_object.property("added").unwrap().ty, wk.string);

    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [DiagnosticCode::TK2428, DiagnosticCode::TK2428]
    );
    assert!(result.incomplete.is_empty(), "{:?}", result.incomplete);
}

#[test]
fn class_interface_groups_remain_usable_from_other_class_surfaces() {
    let mut missing_name_cases = Vec::new();
    let mut obsolete_incomplete_cases = Vec::new();
    for source in [
        "class Mixed {} interface Mixed { value: number } declare class Consumer { field: Mixed }",
        "declare namespace N { class Mixed {} interface Mixed { value: number } } declare class Consumer { field: N.Mixed }",
    ] {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let mut interner = Interner::with_intrinsics();

        let result = check_program_with_publication_inspector(
            &mut interner,
            &parsed.program,
            |binder, environment, _| {
                let consumer = published_group(binder, environment, "Consumer");
                let PublishedTypeGroupSurface::Class(class) = consumer.surface else {
                    panic!("Consumer is a class endpoint")
                };
                assert!(matches!(
                    environment.classes().published_class(class),
                    crate::class_semantics::DemandOutcome::Ready(_)
                ));
            },
        );

        if result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::TK2304)
        {
            missing_name_cases.push((source, result.diagnostics));
        }
        if result.incomplete.iter().any(|incomplete| {
            incomplete.id == "annotation-lower/type-name/qualified-name"
        }) {
            obsolete_incomplete_cases.push((source, result.incomplete));
        }
    }
    assert!(
        missing_name_cases.is_empty() && obsolete_incomplete_cases.is_empty(),
        "found unavailable failures: missing names {missing_name_cases:?}; obsolete WU2 incompletes {obsolete_incomplete_cases:?}"
    );
}

#[test]
fn class_owned_interface_cycle_publishes_asymmetric_ready_surfaces() {
    let source = "\
class A { ownA: number = 1; }
interface A extends B {}
class B { ownB: string = \"b\"; }
interface B extends A {}
";
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut interner = Interner::with_intrinsics();

    let result = check_program_with_publication_inspector(
        &mut interner,
        &parsed.program,
        |binder, environment, interner| {
            let a = published_group(binder, environment, "A");
            let b = published_group(binder, environment, "B");
            let PublishedTypeGroupSurface::Class(a_class) = a.surface else {
                panic!("A keeps its class identity")
            };
            let PublishedTypeGroupSurface::Class(b_class) = b.surface else {
                panic!("B keeps its class identity")
            };
            let DemandOutcome::Ready(a_surface) = environment.classes().published_class(a_class)
            else {
                panic!("A publishes a recovered surface")
            };
            let DemandOutcome::Ready(b_surface) = environment.classes().published_class(b_class)
            else {
                panic!("B publishes a recovered surface")
            };
            let a_object = interner
                .store()
                .object_type(a_surface.instance_template())
                .expect("A instance object");
            assert!(a_object.property("ownA").is_some());
            assert!(a_object.property("ownB").is_none());
            let b_object = interner
                .store()
                .object_type(b_surface.instance_template())
                .expect("B instance object");
            assert!(b_object.property("ownA").is_some());
            assert!(b_object.property("ownB").is_some());
            assert_published_types_are_owner_closed(interner.store(), a);
            assert_published_types_are_owner_closed(interner.store(), b);
        },
    );
    assert_eq!(
        result
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == DiagnosticCode::TK2310)
            .count(),
        4
    );
    assert!(result.incomplete.is_empty(), "{:?}", result.incomplete);
}

#[test]
fn cyclic_heritage_keeps_own_and_external_members_without_crossing_binders() {
    for source in [
        "\
interface External<X> { external: X }
interface A<T> extends External<T>, B<T> { a: T; wrapped: { inner: T[] } }
interface B<U> extends A<U> { b: U }
",
        "\
interface External<X> { external: X }
interface B<U> extends A<U> { b: U }
interface A<T> extends External<T>, B<T> { a: T; wrapped: { inner: T[] } }
",
    ] {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let mut interner = Interner::with_intrinsics();

        let result = check_program_with_publication_inspector(
            &mut interner,
            &parsed.program,
            |binder, environment, interner| {
                let a = published_group(binder, environment, "A");
                let b = published_group(binder, environment, "B");
                let PublishedTypeGroupSurface::Template(a_root) = a.surface else {
                    panic!("A is an interface template")
                };
                let PublishedTypeGroupSurface::Template(b_root) = b.surface else {
                    panic!("B is an interface template")
                };
                let a_object = interner.store().object_type(a_root).expect("A object");
                let b_object = interner.store().object_type(b_root).expect("B object");

                assert_eq!(
                    a_object
                        .properties
                        .iter()
                        .map(|property| property.name.as_str())
                        .collect::<Vec<_>>(),
                    vec!["a", "external", "wrapped"]
                );
                assert_eq!(
                    b_object
                        .properties
                        .iter()
                        .map(|property| property.name.as_str())
                        .collect::<Vec<_>>(),
                    vec!["b"]
                );
                assert_published_types_are_owner_closed(interner.store(), a);
                assert_published_types_are_owner_closed(interner.store(), b);
            },
        );
        assert_eq!(
            result
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == DiagnosticCode::TK2310)
                .count(),
            2
        );
    }
}

#[test]
fn deferred_indexed_heritage_alternatives_remain_distinct_until_identity_query() {
    let source = "\
interface Left { f<T extends { value: string; other: number }>(): T[\"value\"] }
interface Right { f<U extends { value: string; other: number }>(): U[\"other\"] }
interface Derived extends Left, Right {}
";
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut interner = Interner::with_intrinsics();

    let result = check_program_with_publication_inspector(
        &mut interner,
        &parsed.program,
        |binder, environment, interner| {
            let derived = published_group(binder, environment, "Derived");
            let alternative = derived
                .conflict_alternatives
                .iter()
                .find(|alternative| {
                    alternative.kind == InterfaceAlternativeKind::Heritage && alternative.key == "f"
                })
                .expect("indexed return mismatch remains a typed heritage alternative");
            assert_eq!(alternative.types.len(), 2);
            for ty in &alternative.types {
                let function = interner
                    .store()
                    .function_type(*ty)
                    .expect("method alternative is a function");
                assert_eq!(
                    interner.store().tag(function.ret),
                    TypeTag::DeferredIndexedAccess
                );
            }
        },
    );
    assert_eq!(
        result
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == DiagnosticCode::TK2320)
            .count(),
        1
    );
}

#[test]
fn implicit_any_class_properties_keep_member_order_span_and_cardinality() {
    let source = "class C { public publicValue; private privateValue; protected protectedValue; static staticValue; readonly readonlyValue; }";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(
        output
            .incomplete
            .iter()
            .map(|incomplete| (
                incomplete.id.as_str(),
                incomplete.context.as_str(),
                &source[incomplete.span.range()],
            ))
            .collect::<Vec<_>>(),
        [
            (
                "class/property-definition/implicit-any",
                "class property without an annotation or initializer has implicit any type",
                "public publicValue;",
            ),
            (
                "class/property-definition/implicit-any",
                "class property without an annotation or initializer has implicit any type",
                "private privateValue;",
            ),
            (
                "class/property-definition/implicit-any",
                "class property without an annotation or initializer has implicit any type",
                "protected protectedValue;",
            ),
            (
                "class/property-definition/implicit-any",
                "class property without an annotation or initializer has implicit any type",
                "static staticValue;",
            ),
            (
                "class/property-definition/implicit-any",
                "class property without an annotation or initializer has implicit any type",
                "readonly readonlyValue;",
            ),
        ]
    );
}

#[test]
fn explicit_any_class_properties_remain_complete_and_published() {
    let source = "\
class Complete {
  public publicValue: any;
  private privateValue: any;
  protected protectedValue: any;
  static staticValue: any;
  readonly readonlyValue: any;
}
const value = new Complete();
value.publicValue = 1;
Complete.staticValue = \"ready\";
";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
}

#[test]
fn implicit_any_member_makes_the_whole_class_surface_unavailable() {
    let source = "\
class Poisoned { missing; known: number; }
interface Source { known: number; }
declare const source: Source;
const target: Poisoned = source;
";
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut interner = Interner::with_intrinsics();

    let output = check_program_with_publication_inspector(
        &mut interner,
        &parsed.program,
        |binder, environment, _| {
            let poisoned = published_group(binder, environment, "Poisoned");
            let PublishedTypeGroupSurface::Class(class) = poisoned.surface else {
                panic!("Poisoned keeps its class identity")
            };
            assert!(matches!(
                environment.classes().published_class(class),
                DemandOutcome::Exhausted(Exhaustion::ClassSurfacePoison { class: poisoned })
                    if poisoned == class
            ));
        },
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(
        output
            .incomplete
            .iter()
            .map(|incomplete| incomplete.id.as_str())
            .collect::<Vec<_>>(),
        ["class/property-definition/implicit-any"]
    );
}

#[test]
fn implicit_any_class_interface_reopening_is_poisoned_in_both_orders() {
    for source in [
        "class Mixed { missing; known: number; }\ninterface Mixed { reopened: string; }",
        "interface Mixed { reopened: string; }\nclass Mixed { missing; known: number; }",
    ] {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let mut interner = Interner::with_intrinsics();

        let output = check_program_with_publication_inspector(
            &mut interner,
            &parsed.program,
            |binder, environment, _| {
                let mixed = published_group(binder, environment, "Mixed");
                let PublishedTypeGroupSurface::Class(class) = mixed.surface else {
                    panic!("Mixed keeps its class identity")
                };
                assert!(matches!(
                    environment.classes().published_class(class),
                    DemandOutcome::Exhausted(Exhaustion::ClassSurfacePoison { class: poisoned })
                        if poisoned == class
                ));
            },
        );
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(
            output
                .incomplete
                .iter()
                .map(|incomplete| incomplete.id.as_str())
                .collect::<Vec<_>>(),
            ["class/property-definition/implicit-any"]
        );
    }
}

#[test]
fn project_mode_preserves_implicit_any_class_poison_in_the_declaring_module() {
    let reports = check_project(vec![
        FileInput {
            name: "use.ts".into(),
            source: "import { Poisoned } from './class';\ninterface Source { known: number; }\ndeclare const source: Source;\nconst target: Poisoned = source;".into(),
        },
        FileInput {
            name: "class.ts".into(),
            source: "export class Poisoned { missing; known: number; }".into(),
        },
    ]);
    assert!(
        reports
            .iter()
            .all(|report| report.output.parse_errors.is_empty()),
        "unexpected parse errors"
    );
    assert!(
        reports
            .iter()
            .all(|report| report.output.diagnostics.is_empty()),
        "unexpected diagnostics"
    );
    assert!(reports[0].output.incomplete.is_empty());
    assert_eq!(
        reports[1]
            .output
            .incomplete
            .iter()
            .map(|incomplete| incomplete.id.as_str())
            .collect::<Vec<_>>(),
        ["class/property-definition/implicit-any"]
    );
}

#[test]
fn official_private_identity_case_is_not_a_false_clean_class_surface() {
    let source = "\
// @target: es2015
interface T1 { }
interface T2 { z }

class C1<T> {
    private x;
}

class C2 extends C1<T1> {
    y;
}

var c1: C1<T2>;
<C2>c1;


class C3<T> {
    private x: T;
}

class C4 extends C3<T1> {
    y;
}

var c3: C3<T2>;
<C4>c3;
";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let lines = crate::span::LineIndex::new(source);
    assert_eq!(
        output
            .incomplete
            .iter()
            .map(|incomplete| (
                lines.line_of(incomplete.span.start),
                incomplete.id.as_str(),
                &source[incomplete.span.range()],
            ))
            .collect::<Vec<_>>(),
        [
            (6, "class/property-definition/implicit-any", "private x;"),
            (9, "class/class-heritage/poisoned-base", "C1"),
            (10, "class/property-definition/implicit-any", "y;"),
            (22, "class/property-definition/implicit-any", "y;"),
        ]
    );
}

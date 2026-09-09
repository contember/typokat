use super::*;

#[test]
fn assignment_fixed_arity_preserves_excess_for_long_and_short_tuples() {
    for target in ["[{value: number}]", "[{value: number}, number]"] {
        let value = if target == "[{value: number}]" {
            "[{extra: true}, 1]"
        } else {
            "[{extra: true}]"
        };
        let source = format!("declare let target: {target};\ntarget = {value};");
        let output = check_source(&source);
        assert!(output.parse_errors.is_empty());
        assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
        let index = crate::span::LineIndex::new(&source);
        let actual = output
            .diagnostics
            .iter()
            .map(|diagnostic| {
                let position = index.line_col(diagnostic.span.start);
                (
                    position.line,
                    position.column,
                    diagnostic.code.as_str(),
                    &source[diagnostic.span.range()],
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, [(2, 12, "TK2353", "extra")], "{source}");
    }
}

#[test]
fn assignment_fixed_arity_six_markers_keep_child_and_length_owners() {
    let source = include_str!(
        "../../../../../../tests/cases/b44_as_const/assignment_tuple_arity_diagnostic_targets.ts"
    );
    let output = check_source(source);
    assert!(output.parse_errors.is_empty());
    assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
    let index = crate::span::LineIndex::new(source);
    let actual = output
        .diagnostics
        .iter()
        .map(|diagnostic| {
            let position = index.line_col(diagnostic.span.start);
            (
                position.line,
                position.column,
                diagnostic.code.as_str(),
                &source[diagnostic.span.range()],
            )
        })
        .collect::<Vec<_>>();
    println!("fixed_arity_diagnostics: {:?}", output.diagnostics);
    println!("fixed_arity_positions: {actual:?}");
    assert_eq!(
        actual,
        [
            (2, 32, "TK2353", "extra"),
            (3, 29, "TK2322", "[{ value: 1 }, 1]"),
            (7, 33, "TK2353", "extra"),
            (8, 30, "TK2322", "[{ value: 1 }]"),
            (12, 34, "TK2353", "extra"),
            (13, 31, "TK2322", "[{ value: 1 }]"),
        ]
    );
}

#[test]
fn assignment_fixed_arity_known_prefix_before_rest_keeps_child_and_length_owners() {
    let source = "declare let prefixTarget: [{ value: number }, ...string[], boolean];\nprefixTarget = [{ extra: true }];\nprefixTarget = [{ value: 1 }];\nprefixTarget = [{ value: 1 }, true];\n";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty());
    assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
    let index = crate::span::LineIndex::new(source);
    let actual = output
        .diagnostics
        .iter()
        .map(|diagnostic| {
            let position = index.line_col(diagnostic.span.start);
            (
                position.line,
                position.column,
                diagnostic.code.as_str(),
                &source[diagnostic.span.range()],
            )
        })
        .collect::<Vec<_>>();
    println!("fixed_prefix_diagnostics: {:?}", output.diagnostics);
    println!("fixed_prefix_positions: {actual:?}");
    assert_eq!(
        actual,
        [
            (2, 19, "TK2353", "extra"),
            (3, 16, "TK2322", "[{ value: 1 }]"),
        ]
    );
}

#[test]
fn assignment_rest_position_clean_prefix_and_leading_suffixes() {
    let source = "declare let target: [number, ...string[], boolean]; target = [1, true]; target = [1, 'ok', true]; target = [1, 'one', 'two', true]; declare let leading: [...string[], boolean]; leading = [true]; leading = ['ok', true]; leading = ['one', 'two', true];";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty());
    assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn assignment_rest_position_committed_eight_errors_and_clean_controls() {
    let source = include_str!(
        "../../../../../../tests/cases/b44_as_const/assignment_tuple_rest_diagnostic_positions.ts"
    );
    let output = check_source(source);
    assert!(output.parse_errors.is_empty());
    assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
    let index = crate::span::LineIndex::new(source);
    let actual = output
        .diagnostics
        .iter()
        .map(|diagnostic| {
            let position = index.line_col(diagnostic.span.start);
            (position.line, position.column, diagnostic.code.as_str())
        })
        .collect::<Vec<_>>();
    println!("rest_position_diagnostics: {:?}", output.diagnostics);
    println!("rest_position_positions: {actual:?}");
    assert_eq!(
        actual,
        [
            (5, 26, "TK2322"),
            (6, 32, "TK2322"),
            (12, 30, "TK2322"),
            (13, 36, "TK2322"),
            (18, 34, "TK2322"),
            (19, 48, "TK2322"),
            (24, 38, "TK2322"),
            (25, 52, "TK2322")
        ]
    );
}

#[test]
fn assignment_rest_position_preparation_preserves_skipped_and_failed_slots() {
    for initializer in ["[1, 'ok', ,]", "[1, , true]", "[1, 'ok', /unmodeled/]"] {
        let source = format!("class C {{ field = {initializer}; }}");
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, &source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty());
        let binder = crate::binder::bind_module_with_prelude(&parsed.program, &parsed.program);
        let mut interner = Interner::with_intrinsics();
        let mut pass = crate::check::checker::build_pass(
            &mut interner,
            &binder,
            Vec::new(),
            vec![None; binder.type_groups.len()],
            DeclTypes::new(binder.decl_count),
            0,
        );
        pass.type_environment = TypeEnvironmentState::Published(PublishedTypeEnvironment::empty());
        let manifest =
            crate::binder::declaration::SourceDeclarationManifest::build(&parsed.program);
        let (reporting, _) =
            crate::check::checker::reserve_internal_reporting_with_declaration_manifest(
                &parsed.program,
                &manifest,
                crate::source::ModuleOrdinal::new(0),
                crate::source::UnitSlot::new(0),
                None,
            );
        let member = reporting.lexical_events.classes()[0].members[0];
        let ticket = reporting
            .lexical_events
            .member(member)
            .unwrap()
            .tickets
            .immediate;
        let oxc_ast::ast::Statement::ClassDeclaration(class) = &parsed.program.body[0] else {
            panic!("class")
        };
        let oxc_ast::ast::ClassElement::PropertyDefinition(property) = &class.body.body[0] else {
            panic!("field")
        };
        let expression = property.value.as_ref().unwrap();
        pass.with_ticket_effects(ticket, |pass| {
            let wk = pass.interner.well_known();
            let rest = pass.interner.intern_array(wk.string);
            let target = pass
                .interner
                .intern_tuple_type(crate::types::repr::TupleType::with_rest(
                    vec![wk.number, wk.boolean],
                    crate::types::repr::TupleRestType::new(1, rest),
                ));
            let checked = pass.infer_checked_expr(binder.module, expression).unwrap();
            let group = pass.prepare_literal_assignment(expression, &checked, target);
            let expected = if initializer == "[1, , true]" {
                vec![wk.number, wk.boolean]
            } else {
                vec![wk.number, wk.string]
            };
            assert_eq!(
                group
                    .root
                    .children
                    .iter()
                    .map(|site| site.residual.tgt)
                    .collect::<Vec<_>>(),
                expected,
                "{initializer}"
            );
            if initializer.contains("unmodeled") {
                assert_eq!(group.unavailable_sites.len(), 1);
                assert_eq!(group.unavailable_sites[0].target, wk.boolean);
            } else {
                assert!(group.unavailable_sites.is_empty());
                assert_eq!(checked.literal.as_ref().unwrap().children.len(), 2);
            }
        });
    }
}

#[test]
fn assignment_literal_logical_contributions_and_excluded_effects() {
    let source = include_str!(
        "../../../../../../tests/cases/b44_as_const/assignment_logical_literal_sources.ts"
    );
    let output = check_source(source);
    assert!(output.parse_errors.is_empty());
    assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
    let index = crate::span::LineIndex::new(source);
    let actual = output
        .diagnostics
        .iter()
        .map(|diagnostic| {
            (
                index.line_of(diagnostic.span.start),
                diagnostic.code.as_str(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, [(6, "TK2322"), (17, "TK2345")]);
}

#[test]
fn assignment_literal_logical_left_conditional_arms_use_only_surviving_values() {
    let source = "declare const choice: boolean; declare let target: {value: number}; target = (choice ? undefined : {value: 1}) ?? {value: 2}; target = (choice ? false : {value: 1}) || {value: 2};";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty());
    assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn assignment_nested_ownership_requires_all_twenty_site_diagnostics() {
    let source = include_str!(
        "../../../../../../tests/cases/b44_as_const/assignment_nested_diagnostic_ownership.ts"
    );
    let output = check_source(source);
    assert!(output.parse_errors.is_empty());
    assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
    let index = crate::span::LineIndex::new(source);
    let mut actual = output
        .diagnostics
        .iter()
        .map(|d| (index.line_of(d.span.start), d.code.as_str()))
        .collect::<Vec<_>>();
    actual.sort();
    assert_eq!(
        actual,
        [
            (2, "TK2353"),
            (3, "TK2741"),
            (4, "TK2741"),
            (7, "TK2353"),
            (10, "TK2353"),
            (13, "TK2322"),
            (13, "TK2353"),
            (14, "TK2322"),
            (14, "TK2353"),
            (15, "TK2353"),
            (15, "TK2353"),
            (18, "TK2322"),
            (18, "TK2353"),
            (19, "TK2322"),
            (19, "TK2353"),
            (20, "TK2322"),
            (20, "TK2353"),
            (23, "TK2345"),
            (23, "TK2353"),
            (23, "TK2353"),
        ]
    );
}

#[test]
fn assignment_nested_ownership_array_sites_select_their_own_primary() {
    let source = "declare let target: {value: number}[];\ntarget = [{value: 1, extra: true}, {value: 'wrong', extra: true}];\ntarget = [{value: 'wrong', extra: true}, {value: 1, extra: true}];";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty());
    assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
    let index = crate::span::LineIndex::new(source);
    let mut actual = output
        .diagnostics
        .iter()
        .map(|d| (index.line_of(d.span.start), d.code.as_str()))
        .collect::<Vec<_>>();
    actual.sort();
    assert_eq!(
        actual,
        [(2, "TK2322"), (2, "TK2353"), (3, "TK2322"), (3, "TK2353")]
    );
}

#[test]
fn assignment_nested_ownership_array_source_keeps_positional_types_despite_equal_roots() {
    let allocator = Allocator::default();
    let source = "[{value: 1, extra: true}, {value: 'wrong', extra: true}]; [{value: 'wrong', extra: true}, {value: 1, extra: true}];";
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    assert!(parsed.diagnostics.is_empty());
    let binder = crate::binder::bind_module_with_prelude(&parsed.program, &parsed.program);
    let mut interner = Interner::with_intrinsics();
    let mut pass = crate::check::checker::build_pass(
        &mut interner,
        &binder,
        Vec::new(),
        vec![None; binder.type_groups.len()],
        DeclTypes::new(binder.decl_count),
        0,
    );
    pass.type_environment = TypeEnvironmentState::Published(PublishedTypeEnvironment::empty());
    let number = pass.interner.well_known().number;
    let element = pass.interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", number)],
        ..Default::default()
    });
    let target = pass.interner.intern_array(element);
    let mut raw_types = Vec::new();
    let mut contextual_types = Vec::new();
    let mut products = Vec::new();
    for statement in &parsed.program.body {
        let oxc_ast::ast::Statement::ExpressionStatement(statement) = statement else {
            panic!("expression")
        };
        let raw = pass
            .infer_checked_expr(binder.module, &statement.expression)
            .unwrap();
        raw_types.push(raw.ty);
        let contextual = pass
            .infer_initializer_value(
                binder.module,
                &statement.expression,
                Some(target),
                crate::check::checker::expr::ContextualLiteralProvenance::Existing,
            )
            .unwrap();
        contextual_types.push(contextual.source);
        products.push((raw, contextual.checked));
    }
    assert_eq!(raw_types.len(), 2);
    assert_eq!(raw_types[0], raw_types[1]);
    assert_eq!(contextual_types[0], contextual_types[1]);
    let array = pass
        .interner
        .store()
        .array_type(contextual_types[0])
        .unwrap();
    assert_eq!(
        pass.interner
            .store()
            .union_members(array.element)
            .unwrap()
            .len(),
        2
    );
    assert!(pass
        .interner
        .store()
        .tuple_type(contextual_types[0])
        .is_none());
    assert!(pass.satisfies_check_sinks.is_empty());
    assert!(pass.class_initializer_validation.is_empty());
    for (first, second) in [
        (&products[0].0, &products[1].0),
        (&products[0].1, &products[1].1),
    ] {
        let first = first.literal.as_ref().unwrap();
        let second = second.literal.as_ref().unwrap();
        assert_eq!(first.children.len(), 2);
        assert_eq!(second.children.len(), 2);
        for (index, child) in first.children.iter().enumerate() {
            let reversed = &second.children[1 - index];
            assert_eq!(
                child.checked.as_ref().unwrap().ty,
                reversed.checked.as_ref().unwrap().ty
            );
            assert_eq!(child.contribution, reversed.contribution);
            assert_ne!(
                child.checked.as_ref().unwrap().ty,
                second.children[index].checked.as_ref().unwrap().ty
            );
            assert_ne!(child.full_span, reversed.full_span);
            assert!(matches!(child.edge, LiteralEdge::Element(found) if found == index));
        }
    }
}

#[test]
fn assignment_boundaries_boolean_union_members_keep_exact_and_broad_sources() {
    with_pass(|pass, _, _| {
        let wk = pass.interner.well_known();
        let one = pass.interner.intern_literal(LiteralValue::Number(1.0));
        for members in [vec![wk.boolean, one], vec![one, wk.boolean]] {
            let declared = pass.interner.union(members);
            for polarity in [false, true] {
                let exact = pass
                    .interner
                    .intern_literal(LiteralValue::Boolean(polarity));
                let fresh = pass
                    .interner
                    .intern_ordinary_literal(LiteralValue::Boolean(polarity));
                assert_eq!(reduce(pass, declared, exact), exact);
                assert_eq!(reduce(pass, declared, fresh), fresh);
                let mixed = pass.interner.union(vec![exact, one]);
                assert_eq!(reduce(pass, declared, mixed), mixed);
            }
            assert_eq!(reduce(pass, declared, wk.boolean), wk.boolean);
        }
    });
}

#[test]
fn assignment_boundaries_excess_selects_primary_without_losing_rhs_errors() {
    let source = "declare const source: {value: number};\nlet target: {value: number} | {tag: string} = {tag: 'other'};\ntarget = source;\nlet copy = target;\ncopy = {tag: 'other'};\ncopy = {};\ncopy = {value: 1, extra: true};\ncopy = {value: 'wrong', extra: true};\ndeclare function needNumber(value: number): number;\ncopy = {tag: needNumber('wrong')};";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty());
    assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
    let index = crate::span::LineIndex::new(source);
    let mut actual = output
        .diagnostics
        .iter()
        .map(|d| (index.line_of(d.span.start), d.code.as_str()))
        .collect::<Vec<_>>();
    actual.sort();
    assert_eq!(
        actual,
        [
            (5, "TK2353"),
            (6, "TK2741"),
            (7, "TK2353"),
            (8, "TK2322"),
            (10, "TK2345"),
            (10, "TK2353")
        ]
    );
}
use crate::binder::bind::ValueResolution;
use crate::check::checker::type_groups::{PublishedTypeEnvironment, TypeEnvironmentState};
use crate::check::flow::NarrowOp;
use crate::check::query::{query_demand_measure, reset_query_demand_measure, SemanticQueryState};
use crate::check::test_support::check_source;
use crate::class_semantics::{
    ClassConstructionState, Exhaustion, PublishedClassPoison, PublishedClasses,
};
use crate::types::repr::{ClassId, ConditionalType, LiteralValue, ObjectType, PropertyType};
use crate::types::Interner;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

fn with_pass(test: impl FnOnce(&mut Pass<'_, '_>, SymbolId, SymbolId)) {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, "let x; let y;", SourceType::ts()).parse();
    assert!(parsed.diagnostics.is_empty());
    let binder = crate::binder::bind_module_with_prelude(&parsed.program, &parsed.program);
    let mut interner = Interner::with_intrinsics();
    let mut pass = crate::check::checker::build_pass(
        &mut interner,
        &binder,
        Vec::new(),
        vec![None; binder.type_groups.len()],
        DeclTypes::new(binder.decl_count),
        0,
    );
    pass.type_environment = TypeEnvironmentState::Published(PublishedTypeEnvironment::empty());
    let ValueResolution::Resolved { symbol: x, .. } =
        pass.resolve_value_binding(binder.module, "x")
    else {
        panic!("x binding")
    };
    let ValueResolution::Resolved { symbol: y, .. } =
        pass.resolve_value_binding(binder.module, "y")
    else {
        panic!("y binding")
    };
    test(&mut pass, x, y);
}

fn declare(pass: &mut Pass<'_, '_>, symbol: SymbolId, ty: TypeId) {
    let storage = pass.effective_value_storage_for_symbol(symbol).unwrap();
    pass.decl_types.set(storage, ty);
}

fn reduce(pass: &mut Pass<'_, '_>, declared: TypeId, source: TypeId) -> TypeId {
    let result = assignment_reduce_query(
        pass.interner,
        pass.type_environment.published(),
        &mut pass.semantic_queries,
        &mut pass.next_type_param,
        declared,
        source,
    );
    match result {
        SemanticQueryAttempt::Complete(DemandOutcome::Ready(ty)) => ty,
        other => panic!("{other:?}"),
    }
}

#[test]
fn assignment_flow_preserves_nonunion_ids_and_declared_mutable_members() {
    with_pass(|pass, _, _| {
        let wk = pass.interner.well_known();
        let one = pass.interner.intern_literal(LiteralValue::Number(1.0));
        let two = pass.interner.intern_literal(LiteralValue::Number(2.0));
        assert_eq!(reduce(pass, wk.number, one), wk.number);
        assert_eq!(reduce(pass, one, two), one);
        let a = pass
            .interner
            .intern_literal(LiteralValue::String("a".into()));
        let b = pass
            .interner
            .intern_literal(LiteralValue::String("b".into()));
        let mutable = pass.interner.intern_object(ObjectType {
            properties: vec![
                PropertyType::public("kind", a),
                PropertyType::public("value", wk.number),
            ],
            ..Default::default()
        });
        let other = pass.interner.intern_object(ObjectType {
            properties: vec![
                PropertyType::public("kind", b),
                PropertyType::public("value", wk.string),
            ],
            ..Default::default()
        });
        let mut kind = PropertyType::public("kind", a);
        kind.readonly = true;
        let mut value = PropertyType::public("value", one);
        value.readonly = true;
        let source = pass.interner.intern_object(ObjectType {
            properties: vec![kind, value],
            ..Default::default()
        });
        assert_eq!(reduce(pass, mutable, source), mutable);
        for members in [vec![mutable, other], vec![other, mutable]] {
            let union = pass.interner.union(members);
            assert_eq!(reduce(pass, union, source), mutable);
        }
        assert!(
            !pass
                .interner
                .store()
                .object_type(mutable)
                .unwrap()
                .property("value")
                .unwrap()
                .readonly
        );
        assert!(
            pass.interner
                .store()
                .object_type(source)
                .unwrap()
                .property("value")
                .unwrap()
                .readonly
        );
    });
}

#[test]
fn assignment_flow_requires_whole_source_coverage_in_both_orders() {
    with_pass(|pass, _, _| {
        let one = pass.interner.intern_literal(LiteralValue::Number(1.0));
        let two = pass.interner.intern_literal(LiteralValue::Number(2.0));
        let three = pass.interner.intern_literal(LiteralValue::Number(3.0));
        for targets in [vec![one, two], vec![two, one]] {
            let declared = pass.interner.union(targets);
            for sources in [vec![one, three], vec![three, one]] {
                let source = pass.interner.union(sources);
                assert_eq!(reduce(pass, declared, source), declared);
            }
            assert_eq!(reduce(pass, declared, three), declared);
            assert_eq!(reduce(pass, declared, two), two);
        }
    });
}

#[test]
fn assignment_flow_retains_boolean_modes_and_union_never() {
    with_pass(|pass, _, _| {
        let wk = pass.interner.well_known();
        let ordinary = pass
            .interner
            .intern_ordinary_literal(LiteralValue::Boolean(false));
        let exact = pass.interner.intern_literal(LiteralValue::Boolean(false));
        let one = pass.interner.intern_literal(LiteralValue::Number(1.0));
        for declared in [wk.boolean, pass.interner.union(vec![exact, one])] {
            assert_eq!(reduce(pass, declared, ordinary), ordinary);
            assert_eq!(reduce(pass, declared, exact), exact);
            assert_eq!(reduce(pass, declared, wk.never), wk.never);
        }
        assert_eq!(pass.interner.widen_literal_type(ordinary), wk.boolean);
        assert_eq!(pass.interner.widen_literal_type(exact), exact);
        assert_eq!(reduce(pass, wk.number, wk.never), wk.number);
    });
}

#[test]
fn assignment_flow_preflights_poison_before_nonunion_identity_and_never() {
    let mut interner = Interner::with_intrinsics();
    let class = ClassId(17);
    let application = interner.intern_class_instance(class, Vec::new());
    let object = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("nested", application)],
        ..Default::default()
    });
    let published = PublishedClasses::from_publication(
        FxHashMap::from_iter([(class, ClassConstructionState::Poisoned)]),
        FxHashMap::default(),
        FxHashMap::from_iter([(class, PublishedClassPoison::Surface)]),
    )
    .unwrap();
    let mut state = SemanticQueryState::default();
    let mut next = 0;
    let wk = interner.well_known();
    for (declared, source) in [
        (object, object),
        (object, wk.never),
        (object, wk.number),
        (wk.number, object),
    ] {
        let before = state.durable_lengths();
        assert!(matches!(assignment_reduce_query(
            &mut interner, &published, &mut state, &mut next, declared, source,
        ), SemanticQueryAttempt::Complete(DemandOutcome::Exhausted(Exhaustion::ClassSurfacePoison { class: poisoned })) if poisoned == class));
        assert_eq!(state.durable_lengths(), before);
    }
}

fn ready_assignment(pass: &mut Pass<'_, '_>, symbol: SymbolId) -> FlowNodeId {
    let wk = pass.interner.well_known();
    declare(pass, symbol, wk.number);
    let source = pass.interner.intern_conditional(ConditionalType {
        check: wk.number,
        extends_ty: wk.number,
        true_branch: wk.number,
        false_branch: wk.never,
        infer_count: 0,
        distributive: false,
        poisoned: false,
    });
    pass.new_flow(FlowNode::Assignment {
        symbol,
        assigned: Some(source),
        assigned_source: None,
        antecedent: FlowNodeId::START,
    })
}

#[test]
fn assignment_flow_late_failure_discards_recursive_child_and_all_query_writes() {
    with_pass(|pass, x, y| {
        let ready = ready_assignment(pass, y);
        let pending_class = pass.interner.intern_class_instance(ClassId(18), Vec::new());
        declare(pass, x, pending_class);
        let failing = pass.new_flow(FlowNode::Assignment {
            symbol: x,
            assigned: None,
            assigned_source: Some(y),
            antecedent: ready,
        });
        let before = pass.semantic_queries.durable_lengths();
        reset_query_demand_measure();
        assert!(matches!(
            pass.resolve_narrowed_type(failing, x),
            DemandOutcome::Exhausted(_)
        ));
        assert!(query_demand_measure().evaluation_changed_returns > 0);
        assert!(pass.flow_memo.is_empty());
        assert_eq!(pass.semantic_queries.durable_lengths(), before);
        assert_eq!(
            pass.resolve_narrowed_type(ready, y),
            DemandOutcome::Ready(pass.interner.well_known().number)
        );
        assert!(pass.flow_memo.contains_key(&(ready, y)));
        assert_ne!(pass.semantic_queries.durable_lengths(), before);
    });
}

#[test]
fn assignment_flow_failed_condition_propagates_and_loop_cleanup_is_total() {
    for in_loop in [false, true] {
        with_pass(|pass, x, _| {
            let ready = ready_assignment(pass, x);
            let pending_class = pass.interner.intern_class_instance(ClassId(19), Vec::new());
            let bad = pass.new_flow(FlowNode::Assignment {
                symbol: x,
                assigned: Some(pending_class),
                assigned_source: None,
                antecedent: ready,
            });
            let root = if in_loop {
                pass.new_flow(FlowNode::LoopLabel {
                    pre: vec![ready],
                    back: vec![bad],
                })
            } else {
                let storage = pass.effective_value_storage_for_symbol(x).unwrap();
                pass.decl_types.set(storage, pending_class);
                pass.new_flow(FlowNode::Condition {
                    symbol: x,
                    op: NarrowOp::In {
                        property: "field".into(),
                    },
                    positive: true,
                    antecedent: FlowNodeId::START,
                })
            };
            let before = pass.semantic_queries.durable_lengths();
            assert!(matches!(
                pass.resolve_narrowed_type(root, x),
                DemandOutcome::Exhausted(_)
            ));
            assert!(pass.flow_memo.is_empty());
            assert!(pass.flow_provisional.is_empty());
            assert_eq!(pass.flow_loop_depth, 0);
            assert_eq!(pass.semantic_queries.durable_lengths(), before);
        });
    }
}

#[test]
fn assignment_flow_successful_loop_stages_only_discharged_label() {
    with_pass(|pass, x, _| {
        let ready = ready_assignment(pass, x);
        let label = pass.new_flow(FlowNode::LoopLabel {
            pre: vec![ready],
            back: Vec::new(),
        });
        let back = pass.new_flow(FlowNode::Assignment {
            symbol: x,
            assigned: None,
            assigned_source: Some(x),
            antecedent: label,
        });
        pass.add_back_edge(label, back);
        let before = pass.semantic_queries.durable_lengths();
        assert_eq!(
            pass.resolve_narrowed_type(label, x),
            DemandOutcome::Ready(pass.interner.well_known().number)
        );
        assert_eq!(pass.flow_memo.len(), 1);
        assert!(pass.flow_memo.contains_key(&(label, x)));
        assert_eq!(pass.semantic_queries.durable_lengths(), before);
        assert!(pass.flow_provisional.is_empty());
        assert_eq!(pass.flow_loop_depth, 0);
    });
}

#[test]
fn assignment_flow_reports_invalid_write_and_rhs_effect_once() {
    let source = "function take(x: number): 1 { return 1; }\nlet target: 2 | 3 = 2;\ntarget = take('bad');\ntarget = target;";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|d| (d.code.as_str(), &source[d.span.range()]))
            .collect::<Vec<_>>(),
        [("TK2345", "'bad'"), ("TK2322", "take('bad')")]
    );
}

#[test]
fn assignment_flow_compound_reads_target_before_rhs_assignment() {
    let source = "let x: string | number = '';\nx = '';\nlet result = (x += (x = 1));\nresult = 'valid';\nresult = 2;";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|d| (d.code.as_str(), &source[d.span.range()]))
            .collect::<Vec<_>>(),
        [("TK2322", "2")]
    );
}

#[test]
fn assignment_flow_native_retry_drops_child_writes_and_rereads_both_declarations() {
    with_pass(|pass, x, y| {
        let ready = ready_assignment(pass, y);
        let wk = pass.interner.well_known();
        let object = pass.interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("value", wk.number)],
            ..Default::default()
        });
        declare(pass, x, object);
        pass.semantic_queries
            .set_native_identity_availability([true; 8]);
        let root = pass.new_flow(FlowNode::Assignment {
            symbol: x,
            assigned: None,
            assigned_source: Some(y),
            antecedent: ready,
        });
        let before = pass.semantic_queries.durable_lengths();
        reset_query_demand_measure();
        assert!(matches!(pass.resolve_narrowed_type_attempt(root, x),
            SemanticQueryAttempt::NeedsNativeIdentities(demands) if !demands.is_empty()));
        assert!(query_demand_measure().evaluation_changed_returns > 0);
        assert_eq!(pass.semantic_queries.durable_lengths(), before);
        assert!(pass.flow_memo.is_empty());
        assert_eq!(
            pass.resolve_narrowed_type(root, x),
            DemandOutcome::Exhausted(Exhaustion::EvaluationInvalidNode { ty: object })
        );
        assert_eq!(pass.semantic_queries.durable_lengths(), before);
        assert!(pass.flow_memo.is_empty());
        declare(pass, y, wk.string);
        let union = pass.interner.union(vec![wk.number, wk.string]);
        declare(pass, x, union);
        assert_eq!(
            pass.resolve_narrowed_type(root, x),
            DemandOutcome::Ready(wk.string)
        );
        assert_eq!(pass.flow_memo.get(&(ready, y)).unwrap().ty, wk.string);
    });
}

#[test]
fn assignment_flow_real_budget_exhaustion_rolls_back_a_ready_sibling() {
    with_pass(|pass, x, y| {
        let ready = ready_assignment(pass, y);
        let wk = pass.interner.well_known();
        let mut target = wk.string;
        for _ in 0..=crate::check::checker::eval::DEFAULT_STEP_BUDGET {
            target = pass.interner.intern_conditional(ConditionalType {
                check: wk.number,
                extends_ty: wk.number,
                true_branch: target,
                false_branch: wk.never,
                infer_count: 0,
                distributive: false,
                poisoned: false,
            });
        }
        declare(pass, x, target);
        let root = pass.new_flow(FlowNode::Assignment {
            symbol: x,
            assigned: None,
            assigned_source: Some(y),
            antecedent: ready,
        });
        let before = pass.semantic_queries.durable_lengths();
        assert_eq!(
            pass.resolve_narrowed_type(root, x),
            DemandOutcome::Exhausted(Exhaustion::EvaluationBudget)
        );
        assert_eq!(pass.semantic_queries.durable_lengths(), before);
        assert!(pass.flow_memo.is_empty());
    });
}

#[test]
fn assignment_flow_closed_enum_retains_declared_member_narrowing() {
    let source = "enum Mode { Read = 'read', Write = 'write' }\nlet value = Mode.Read;\nvalue = Mode.Write;\nconst exact: Mode.Write = value;\nconst wrong: Mode.Read = value;";
    let output = check_source(source);
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|d| (d.code.as_str(), &source[d.span.range()]))
            .collect::<Vec<_>>(),
        [("TK2322", "wrong")]
    );
}

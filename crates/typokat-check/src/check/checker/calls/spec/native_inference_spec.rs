use super::*;
use crate::check::checker::type_groups::PublishedTypeEnvironment;
use crate::types::repr::{LiteralValue, ObjectType, PropertyType};
use oxc_allocator::Allocator;
use oxc_ast::ast::Statement;
use oxc_parser::Parser;
use oxc_span::SourceType;

fn with_owned_literal(source: &str, test: impl FnOnce(&mut Pass<'_, '_>, &Expression<'_>)) {
    let allocator = Allocator::default();
    let source = format!("class C {{ field = {source}; }}");
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
    let manifest = crate::binder::declaration::SourceDeclarationManifest::build(&parsed.program);
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
    let Statement::ClassDeclaration(class) = &parsed.program.body[0] else {
        panic!("class")
    };
    let oxc_ast::ast::ClassElement::PropertyDefinition(property) = &class.body.body[0] else {
        panic!("field")
    };
    pass.argument_walk_depth = 1;
    pass.with_ticket_effects(ticket, |pass| test(pass, property.value.as_ref().unwrap()));
}

#[test]
fn literal_result_actual_memo_hits_share_children_and_separate_environments() {
    with_owned_literal("[{value: 1}, {value: 'wrong'}]", |pass, expression| {
        let scope = pass.binder.module;
        pass.provisional_argument_effects.push(Vec::new());
        let raw = pass.walk_raw_argument(scope, 2, expression).unwrap();
        let raw_hit = pass.walk_raw_argument(scope, 2, expression).unwrap();
        assert!(Arc::ptr_eq(
            raw.literal.as_ref().unwrap(),
            raw_hit.literal.as_ref().unwrap()
        ));
        let number = pass.interner.well_known().number;
        let target_element = pass.interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("value", number)],
            ..Default::default()
        });
        let target = pass.interner.intern_array(target_element);
        let (first, status) = pass.infer_contextual_checked_source_after_walked_reporting(
            scope,
            expression,
            target,
            raw.clone(),
            false,
            false,
        );
        assert_eq!(status, ContextualRewalk::Rewalked);
        let (hit, status) = pass.infer_contextual_checked_source_after_walked_reporting(
            scope,
            expression,
            target,
            raw.clone(),
            false,
            false,
        );
        assert_eq!(status, ContextualRewalk::Rewalked);
        assert!(Arc::ptr_eq(
            first.literal.as_ref().unwrap(),
            hit.literal.as_ref().unwrap()
        ));
        pass.current_this = Some(number);
        let (changed, status) = pass.infer_contextual_checked_source_after_walked_reporting(
            scope,
            expression,
            target,
            raw.clone(),
            false,
            false,
        );
        assert_eq!(status, ContextualRewalk::Rewalked);
        assert_eq!(first.ty, changed.ty);
        assert!(!Arc::ptr_eq(
            first.literal.as_ref().unwrap(),
            changed.literal.as_ref().unwrap()
        ));
        let changed_raw = pass.walk_raw_argument(scope, 2, expression).unwrap();
        assert_eq!(raw.ty, changed_raw.ty);
        assert!(!Arc::ptr_eq(
            raw.literal.as_ref().unwrap(),
            changed_raw.literal.as_ref().unwrap()
        ));
        let pending = pass
            .interner
            .reserve_enum(crate::types::repr::EnumId(0), "Pending");
        let (failed, status) = pass.infer_contextual_checked_source_after_walked_reporting(
            scope,
            expression,
            pending,
            raw.clone(),
            false,
            false,
        );
        assert_eq!(status, ContextualRewalk::Failed);
        assert!(Arc::ptr_eq(
            raw.literal.as_ref().unwrap(),
            failed.literal.as_ref().unwrap()
        ));
        pass.provisional_argument_effects.pop();
    });
}

#[test]
fn literal_result_kept_raw_shapes_only_the_root_and_keeps_raw_descendants() {
    with_owned_literal("(1)", |pass, expression| {
        let scope = pass.binder.module;
        let raw = pass.infer_checked_expr(scope, expression).unwrap();
        let target = pass.interner.intern_literal(LiteralValue::Number(1.0));
        let environment = ContextualInferenceEnvironment::extend(
            None,
            &[],
            &FxHashMap::default(),
            &FxHashMap::default(),
        );
        let (shaped, status) = pass.with_inference_environment(Some(environment), |pass| {
            pass.infer_contextual_checked_source_after_walked_reporting(
                scope,
                expression,
                target,
                raw.clone(),
                false,
                false,
            )
        });
        assert_eq!(status, ContextualRewalk::KeptRaw);
        assert_eq!(shaped.ty, target);
        assert_ne!(raw.ty, shaped.ty);
        let raw_site = raw.literal.as_ref().unwrap();
        let shaped_site = shaped.literal.as_ref().unwrap();
        assert!(Arc::ptr_eq(&raw_site.children, &shaped_site.children));
        assert_eq!(shaped_site.children[0].checked.as_ref().unwrap().ty, raw.ty);
        assert_eq!(shaped_site.context.unwrap().original_target, target);
        assert!(raw_site.context.is_none());
    });
}

#[test]
fn literal_result_parent_poison_dominates_invalid_children_without_losing_them() {
    with_owned_literal("({value: 1})", |pass, expression| {
        let wk = pass.interner.well_known();
        let class = crate::types::repr::ClassId(17);
        let poison = pass.interner.intern_class_instance(class, Vec::new());
        let span = Span::from_oxc(expression.span());
        let mut root = literal_test_site(poison, poison, span);
        root.children.push(literal_test_site(
            wk.string,
            wk.number,
            Span::new(span.start + 1, span.start + 2),
        ));
        let group = PreparedLiteralAssignment {
            root,
            unavailable_sites: Vec::new(),
        };
        let before = pass.semantic_queries.durable_lengths();
        let outcome =
            crate::check::checker::evaluate_literal_assignment_attempt(pass, &group, &mut None);
        let crate::check::query::SemanticQueryAttempt::Complete(LiteralGroupOutcome::Exhausted {
            diagnostics,
            failures,
            ..
        }) = outcome
        else {
            panic!("publication failure")
        };
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code.as_str(), "TK2322");
        assert_eq!(failures.len(), 1);
        assert!(
            matches!(failures[0].exhaustion, Exhaustion::ClassNotPublished { class: found, .. } if found == class)
        );
        assert_eq!(pass.semantic_queries.durable_lengths(), before);
    });
}

#[test]
fn literal_result_raw_slots_compact_successes_and_keep_ast_rematerialization_index() {
    with_owned_literal("f(/missingLibrary/, {value: 1})", |pass, expression| {
        let Expression::CallExpression(call) = expression else {
            panic!("call")
        };
        pass.provisional_argument_effects.push(Vec::new());
        let scope = pass.binder.module;
        assert!(pass
            .walk_raw_argument(scope, 0, call.arguments[0].as_expression().unwrap())
            .is_none());
        let expression = call.arguments[1].as_expression().unwrap();
        let raw = pass.walk_raw_argument(scope, 1, expression).unwrap();
        assert_eq!(pass.provisional_argument_effects.last().unwrap().len(), 1);
        pass.provisional_argument_effects.pop();
        pass.provisional_argument_effects.push(Vec::new());
        let hit = pass.walk_raw_argument(scope, 1, expression).unwrap();
        assert!(Arc::ptr_eq(
            raw.literal.as_ref().unwrap(),
            hit.literal.as_ref().unwrap()
        ));
        assert!(matches!(
            pass.provisional_argument_effects.last().unwrap().as_slice(),
            [ProvisionalArgumentWalk::Memoized { argument_index: 1 }]
        ));
        pass.rewalk_memoized_raw_arguments(scope, &call.arguments);
        assert!(matches!(
            pass.provisional_argument_effects.last().unwrap().as_slice(),
            [ProvisionalArgumentWalk::Held(_)]
        ));
        pass.provisional_argument_effects.pop();
    });
}

fn literal_test_site(source: TypeId, target: TypeId, span: Span) -> LiteralAssignmentSite {
    LiteralAssignmentSite {
        residual: AssignObligation {
            src: source,
            tgt: target,
            src_span: span,
            source_sites: Vec::new(),
            kind: ObligationKind::Assignment,
        },
        children: Vec::new(),
        excess: Vec::new(),
        decomposed: false,
        unavailable: false,
        preparation_failure: None,
        missing_required: false,
    }
}

#[test]
fn literal_result_late_parent_budget_keeps_sibling_and_rolls_back_queries() {
    with_owned_literal("({left: 'wrong', right: 1})", |pass, expression| {
        let wk = pass.interner.well_known();
        let mut target = wk.string;
        for _ in 0..=crate::check::checker::eval::DEFAULT_STEP_BUDGET {
            target = pass
                .interner
                .intern_conditional(crate::types::repr::ConditionalType {
                    check: wk.number,
                    extends_ty: wk.number,
                    true_branch: target,
                    false_branch: wk.never,
                    infer_count: 0,
                    distributive: false,
                    poisoned: false,
                });
        }
        let parent_span = Span::from_oxc(expression.span());
        let child_span = Span::new(parent_span.start + 2, parent_span.start + 6);
        let mut root = literal_test_site(wk.string, target, parent_span);
        root.children
            .push(literal_test_site(wk.string, wk.number, child_span));
        let group = PreparedLiteralAssignment {
            root,
            unavailable_sites: Vec::new(),
        };
        assert!(matches!(
            pass.is_assignable(wk.number, wk.number),
            RelationOutcome::Yes
        ));
        let before = pass.semantic_queries.durable_lengths();
        let outcome =
            crate::check::checker::evaluate_literal_assignment_attempt(pass, &group, &mut None);
        let crate::check::query::SemanticQueryAttempt::Complete(LiteralGroupOutcome::Exhausted {
            diagnostics,
            failures,
            unavailable_sites,
        }) = outcome
        else {
            panic!("typed failure")
        };
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].span, child_span);
        assert_eq!(diagnostics[0].code.as_str(), "TK2322");
        assert!(unavailable_sites.is_empty());
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].obligation.src_span, parent_span);
        assert_eq!(failures[0].exhaustion, Exhaustion::EvaluationBudget);
        assert_eq!(pass.semantic_queries.durable_lengths(), before);
        let owner = pass.effect_stack.last().unwrap().records.owner();
        let mut effects = CheckerEffects::new(owner);
        crate::check::checker::evaluate_literal_assignment(pass, &mut effects, &group);
        let (_, records) = effects.records.into_parts();
        let diagnostics = records
            .into_iter()
            .filter_map(|record| match record {
                crate::check::checker::reporting_record::CheckerRecord::Diagnostic(diagnostic) => {
                    Some(diagnostic)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            diagnostics
                .iter()
                .map(|diagnostic| (diagnostic.code.as_str(), diagnostic.span))
                .collect::<Vec<_>>(),
            [("TK2322", child_span), ("TK2589", parent_span)]
        );
        assert!(matches!(
            pass.is_assignable(wk.string, wk.number),
            RelationOutcome::No(_)
        ));
    });
}

#[test]
fn literal_result_late_unavailable_source_keeps_prepared_invalid_sibling() {
    with_owned_literal(
        "({left: {extra: true}, right: /unmodeled/})",
        |pass, expression| {
            let wk = pass.interner.well_known();
            let left = pass.interner.intern_object(ObjectType {
                properties: vec![PropertyType::public("value", wk.number)],
                ..Default::default()
            });
            let target = pass.interner.intern_object(ObjectType {
                properties: vec![
                    PropertyType::public("left", left),
                    PropertyType::public("right", wk.number),
                ],
                ..Default::default()
            });
            let checked = pass
                .infer_checked_expr(pass.binder.module, expression)
                .unwrap();
            let before = pass.semantic_queries.durable_lengths();
            let group = pass.prepare_literal_assignment(expression, &checked, target);
            assert_eq!(group.unavailable_sites.len(), 1);
            assert_eq!(group.unavailable_sites[0].target, wk.number);
            assert_eq!(pass.semantic_queries.durable_lengths(), before);
            let outcome =
                crate::check::checker::evaluate_literal_assignment_attempt(pass, &group, &mut None);
            let crate::check::query::SemanticQueryAttempt::Complete(
                LiteralGroupOutcome::Unavailable { diagnostics, sites },
            ) = outcome
            else {
                panic!("unavailable")
            };
            assert_eq!(diagnostics.len(), 1);
            assert_eq!(diagnostics[0].code.as_str(), "TK2353");
            assert_eq!(sites.len(), 1);
            assert_eq!(pass.semantic_queries.durable_lengths(), before);
        },
    );
}

#[test]
fn literal_result_late_target_preparation_failure_keeps_ready_sibling() {
    with_owned_literal("({left: {extra: true}, right: {}})", |pass, expression| {
        let wk = pass.interner.well_known();
        let left = pass.interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("value", wk.number)],
            ..Default::default()
        });
        let pending = pass
            .interner
            .reserve_enum(crate::types::repr::EnumId(0), "Pending");
        let target = pass.interner.intern_object(ObjectType {
            properties: vec![
                PropertyType::public("left", left),
                PropertyType::public("right", pending),
            ],
            ..Default::default()
        });
        let checked = pass
            .infer_checked_expr(pass.binder.module, expression)
            .unwrap();
        let before = pass.semantic_queries.durable_lengths();
        let group = pass.prepare_literal_assignment(expression, &checked, target);
        assert_eq!(group.unavailable_sites.len(), 1);
        assert_eq!(group.unavailable_sites[0].target, pending);
        assert_eq!(pass.semantic_queries.durable_lengths(), before);
        let outcome =
            crate::check::checker::evaluate_literal_assignment_attempt(pass, &group, &mut None);
        let crate::check::query::SemanticQueryAttempt::Complete(LiteralGroupOutcome::Exhausted {
            diagnostics,
            failures,
            unavailable_sites,
        }) = outcome
        else {
            panic!("typed failure")
        };
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code.as_str(), "TK2353");
        assert_eq!(unavailable_sites.len(), 1);
        assert!(!failures.is_empty());
        assert_eq!(pass.semantic_queries.durable_lengths(), before);
    });
}

#[test]
fn literal_result_native_request_rolls_back_then_retries_retained_sites() {
    with_owned_literal("({value: 1})", |pass, expression| {
        let wk = pass.interner.well_known();
        let object = pass.interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("value", wk.number)],
            ..Default::default()
        });
        let span = Span::from_oxc(expression.span());
        let mut root = literal_test_site(wk.string, object, span);
        root.children.push(literal_test_site(
            wk.string,
            wk.number,
            Span::new(span.start + 1, span.start + 2),
        ));
        let group = PreparedLiteralAssignment {
            root,
            unavailable_sites: Vec::new(),
        };
        pass.semantic_queries
            .set_native_identity_availability([true; 8]);
        let before = pass.semantic_queries.durable_lengths();
        let mut request = None;
        assert!(
            matches!(crate::check::checker::evaluate_literal_assignment_attempt(pass, &group, &mut request),
            crate::check::query::SemanticQueryAttempt::NeedsNativeIdentities(demands) if !demands.is_empty())
        );
        assert_eq!(request.unwrap().src_span, span);
        assert_eq!(pass.semantic_queries.durable_lengths(), before);
        pass.semantic_queries
            .set_native_identity_availability([false; 8]);
        let outcome =
            crate::check::checker::evaluate_literal_assignment_attempt(pass, &group, &mut None);
        let crate::check::query::SemanticQueryAttempt::Complete(LiteralGroupOutcome::Decided(
            LiteralDecision::Invalid(diagnostics),
        )) = outcome
        else {
            panic!("invalid retry")
        };
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code.as_str(), "TK2322");
    });
}

#[test]
fn literal_result_root_census_keeps_contributions_and_all_contexts() {
    with_owned_literal("({value: 1})", |pass, expression| {
        let scope = pass.binder.module;
        let raw = pass.infer_checked_expr(scope, expression).unwrap();
        let pending = pass
            .interner
            .reserve_enum(crate::types::repr::EnumId(0), "Pending");
        let wk = pass.interner.well_known();
        let contexts = PreparedInitializerContext {
            original_target: pending,
            publication_context: wk.string,
            shaping_context: wk.number,
            contextual_this: Some(wk.boolean),
        };
        let span = Span::from_oxc(expression.span());
        let checked = pass
            .literal_result(
                scope,
                raw.ty,
                span,
                LiteralKind::Parenthesized,
                vec![LiteralChild {
                    edge: LiteralEdge::Parenthesized,
                    full_span: span,
                    checked: Some(raw.clone()),
                    contribution: Some(pending),
                    required: true,
                }],
            )
            .with_context(Some(contexts));
        let key = pass
            .contextual_walk_key(scope, raw.tuple(), contexts, false)
            .unwrap();
        pass.memoize_contextual_walk(
            key,
            DeclTypes::log_mark(),
            (checked, ContextualRewalk::Rewalked),
        );
        let mut roots = Vec::new();
        pass.append_checked_walk_roots(&mut roots);
        for ty in [pending, wk.string, wk.number, wk.boolean, raw.ty] {
            assert!(roots.contains(&ty));
        }
        assert_eq!(roots.iter().filter(|ty| **ty == pending).count(), 2);
    });
}

#[test]
fn literal_result_product_growth_is_linear_and_clones_share_every_descendant() {
    fn count(value: &CheckedExpr) -> usize {
        value.literal.as_ref().map_or(0, |literal| {
            1 + literal
                .children
                .iter()
                .filter_map(|child| child.checked.as_ref())
                .map(count)
                .sum::<usize>()
        })
    }
    for depth in [1, 3, 7] {
        let source = format!("{}1{}", "[{value: ".repeat(depth), "}]".repeat(depth));
        with_owned_literal(&source, |pass, expression| {
            let checked = pass
                .infer_checked_expr(pass.binder.module, expression)
                .unwrap();
            assert_eq!(count(&checked), depth * 2);
            let clone = checked.clone();
            assert!(Arc::ptr_eq(
                checked.literal.as_ref().unwrap(),
                clone.literal.as_ref().unwrap()
            ));
            assert_eq!(count(&clone), depth * 2);
        });
    }
}

fn with_expression(source: &str, test: impl FnOnce(&mut Pass<'_, '_>, &Expression<'_>)) {
    let allocator = Allocator::default();
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
    let Statement::VariableDeclaration(declaration) = parsed.program.body.last().expect("source")
    else {
        panic!("variable expected");
    };
    test(
        &mut pass,
        declaration.declarations[0]
            .init
            .as_ref()
            .expect("initializer"),
    );
}

#[test]
fn native_independent_annotated_arm_precedes_parameter_fixing() {
    with_expression(
        "const value = true ? x => {} : (x: string) => {};",
        |pass, expression| {
            let scope = pass.binder.module;
            let raw = pass
                .infer_expr(scope, expression)
                .expect("raw expression")
                .0;
            let binder = TypeParamId(100);
            let parameter = pass.interner.intern_type_param(binder, "T");
            let target = pass.interner.intern_function(FunctionType {
                type_params: Vec::new(),
                receiver: None,
                params: vec![ParameterType::required("x", parameter)],
                ret: pass.interner.well_known().void,
            });
            let type_params = vec![GenericTypeParam {
                id: binder,
                constraint: None,
                default: None,
            }];
            assert!(
                infer::inference_position(pass.interner, target, binder)
                    == infer::InferencePosition::CallbackInput
            );
            let mut preparation = infer::CallInferencePreparation::default();
            pass.summarize_inference_shell(
                scope,
                expression,
                ShellInferenceInput {
                    raw: Some(raw),
                    target,
                    root: target,
                    index: 1,
                },
                &type_params,
                &mut preparation,
            );
            assert!(preparation.fixed.contains(&binder));
            assert!(preparation.legacy.is_empty(), "{:?}", preparation.legacy);
            assert!(
                preparation.upper_inputs.contains(&(1, binder)),
                "{:?}",
                preparation.upper_inputs
            );
            assert_eq!(preparation.independent_inputs.len(), 1);
            let literal = pass
                .interner
                .intern_ordinary_literal(LiteralValue::String("read".into()));
            let params = [
                ParameterType::required("value", parameter),
                ParameterType::required("callback", target),
            ];
            preparation.deferred_arguments.insert(1);
            let result = pass.infer_partial_signature_type_arguments_with_native_demands(
                infer::SignatureInferenceRequest {
                    purpose: infer::SignatureInferencePurpose::ContextualPreview {
                        retained_return: parameter,
                        preparation: &preparation,
                    },
                    type_params: &type_params,
                    params: &params,
                    args: &[literal, raw],
                    fresh_args: &[false, true],
                    optional_undefined_fallback: &[false, false],
                    receiver: None,
                },
            );
            let DemandOutcome::Ready(result) = result else {
                panic!("ready preview expected");
            };
            assert_eq!(result.top_level.get(&binder), Some(&false));
            assert_eq!(result.arguments, vec![(binder, literal)]);
        },
    );
}

#[test]
fn native_mapper_content_keys_and_failure_scopes_are_independent() {
    with_expression("const value = 0;", |pass, expression| {
        let wk = pass.interner.well_known();
        let binder = TypeParamId(100);
        let ordinary = FxHashMap::from_iter([(binder, wk.string)]);
        let expected = FxHashMap::from_iter([(binder, wk.number)]);
        let first = ContextualInferenceEnvironment::extend(None, &[], &ordinary, &expected);
        let equal = ContextualInferenceEnvironment::extend(None, &[], &ordinary, &expected);
        assert!(!Arc::ptr_eq(&first, &equal));
        assert_eq!(first, equal);
        pass.argument_walk_depth = 1;
        let scope = pass.binder.module;
        let span = Span::from_oxc(expression.span());
        let key = pass.with_inference_environment(Some(first.clone()), |pass| {
            pass.argument_walk_environment(scope, span)
                .expect("environment")
        });
        let same = pass.with_inference_environment(Some(equal), |pass| {
            pass.argument_walk_environment(scope, span)
                .expect("environment")
        });
        assert!(key == same);
        let different =
            ContextualInferenceEnvironment::extend(None, &[], &ordinary, &FxHashMap::default());
        let changed = pass.with_inference_environment(Some(different), |pass| {
            pass.argument_walk_environment(scope, span)
                .expect("environment")
        });
        assert!(key != changed);
        assert!(pass.inference_environment.is_none());
        let failed: Result<(), Exhaustion> = pass.with_inference_environment(Some(first), |pass| {
            assert!(pass.inference_environment.is_some());
            Err(Exhaustion::EvaluationBudget)
        });
        assert!(failed.is_err());
        assert!(pass.inference_environment.is_none());
    });
}

#[test]
fn native_scaling_counts_receipt() {
    for depth in [2, 4, 8] {
        let mut body = "0".to_owned();
        for index in (0..depth).rev() {
            body = format!("run(v{index} => {body})");
        }
        let source = format!(
            "declare function run<T>(step: (value: number) => T): T; const nested = {body};"
        );
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, &source, SourceType::ts()).parse();
        let mut interner = Interner::with_intrinsics();
        reset_call_measure();
        let checked = crate::check::checker::check_program(&mut interner, &parsed.program);
        assert!(checked.diagnostics.is_empty());
        assert!(checked.incomplete.is_empty());
        let measure = call_measure();
        eprintln!(
            "native depth={depth} contextual={} raw={} callbacks={:?}",
            measure.contextual_argument_walks(),
            measure.raw_call_argument_walks,
            measure.callback_rewalks
        );
    }
}

#[test]
fn native_boundaries_actual_context_product_keeps_parameter_p_before_return_r() {
    with_expression("const source = value => value;", |pass, expression| {
        let wk = pass.interner.well_known();
        let binder = TypeParamId(100);
        let parameter = pass.interner.intern_type_param(binder, "T");
        let one = pass.interner.intern_literal(LiteralValue::Number(1.0));
        let target = pass.interner.intern_function(FunctionType {
            type_params: Vec::new(),
            receiver: None,
            params: vec![ParameterType::required("value", parameter)],
            ret: parameter,
        });
        let environment = ContextualInferenceEnvironment::extend(
            None,
            &[],
            &FxHashMap::from_iter([(binder, one)]),
            &FxHashMap::from_iter([(binder, wk.number)]),
        );
        let mut events = crate::check::checker::events::EventStore::default();
        let owner = events.reserve_event(
            crate::source::ModuleOrdinal::new(0),
            expression.span().start,
        );
        pass.effect_stack.push(CheckerEffects::new(owner.primary));
        pass.argument_walk_depth = 1;
        DeclTypes::open_log();
        pass.with_inference_environment(Some(environment), |pass| {
            let product = pass.infer_contextual_source_after_walked_reporting(
                pass.binder.module,
                expression,
                target,
                (wk.unknown, Span::from_oxc(expression.span())),
                true,
                false,
            );
            assert_eq!(product.1, ContextualRewalk::Rewalked);
            let callable = pass
                .interner
                .store()
                .function_type(product.0 .0)
                .expect("native callable product");
            assert_eq!(callable.params[0].ty, one);
        });
        DeclTypes::close_log();
    });
    for (target, callback) in [
        (
            "(value:T)=>void",
            "value=>{ const exact:1=value; value=2; }",
        ),
        (
            "{run:(value:T)=>void}",
            "{run:value=>{ const exact:1=value; value=2; }}",
        ),
        (
            "[(value:T)=>void]",
            "[value=>{ const exact:1=value; value=2; }]",
        ),
    ] {
        let source = format!("declare function f<T>(v:T,cb:{target}):T; const result:number=f(1 as const,{callback});");
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, &source, SourceType::ts()).parse();
        let mut interner = Interner::with_intrinsics();
        let checked = crate::check::checker::check_program(&mut interner, &parsed.program);
        assert!(checked.incomplete.is_empty());
        assert_eq!(
            checked.diagnostics.len(),
            1,
            "{source}: {:?}",
            checked.diagnostics
        );
        assert_eq!(checked.diagnostics[0].code.as_str(), "TK2322");
        assert!(checked.diagnostics[0]
            .message
            .contains("Type '2' is not assignable to type '1'"));
    }
}

#[test]
fn native_boundaries_literal_union_constraints_shape_returns_before_publication() {
    for (constraint, literal, other) in [
        ("\"read\"|\"write\"", "\"read\"", "\"write\""),
        ("1|2", "1", "2"),
    ] {
        for (target, body) in [
            ("T", literal.to_owned()),
            ("{item:T}", format!("({{item:{literal}}})")),
            ("[T]", format!("[{literal}]")),
        ] {
            let source = format!("declare function f<T extends {constraint}>(cb:()=>{target}):T; const result=f(()=>{body}); const exact:{literal}=result; let copy=result; copy={other};");
            let allocator = Allocator::default();
            let parsed = Parser::new(&allocator, &source, SourceType::ts()).parse();
            let mut interner = Interner::with_intrinsics();
            let checked = crate::check::checker::check_program(&mut interner, &parsed.program);
            assert!(checked.incomplete.is_empty());
            assert_eq!(
                checked.diagnostics.len(),
                1,
                "{source}: {:?}",
                checked.diagnostics
            );
            let diagnostic = &checked.diagnostics[0];
            assert_eq!(diagnostic.code.as_str(), "TK2322");
            assert!(
                source[usize::try_from(diagnostic.span.start).expect("source offset")..]
                    .starts_with(other)
            );
        }
    }
}

#[test]
fn native_boundaries_r_introduces_shapes_without_replacing_existing_parameter_binders() {
    for (parameter, expected, expression) in [
        (
            "T",
            "{callback:(value:number)=>void}",
            "{callback:value=>{const exact:number=value; value=\"bad\";}}",
        ),
        (
            "{callback:T}",
            "{callback:(value:number)=>void}",
            "{callback:value=>{const exact:number=value; value=\"bad\";}}",
        ),
        (
            "T",
            "[(value:number)=>void]",
            "[value=>{const exact:number=value; value=\"bad\";}]",
        ),
        (
            "[T]",
            "[(value:number)=>void]",
            "[value=>{const exact:number=value; value=\"bad\";}]",
        ),
    ] {
        let source = format!("declare function f<T>(value:{parameter}):{parameter}; const result:{expected}=f({expression});");
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, &source, SourceType::ts()).parse();
        let mut interner = Interner::with_intrinsics();
        let checked = crate::check::checker::check_program(&mut interner, &parsed.program);
        assert!(checked.incomplete.is_empty());
        assert_eq!(
            checked.diagnostics.len(),
            1,
            "{source}: {:?}",
            checked.diagnostics
        );
        assert_eq!(checked.diagnostics[0].code.as_str(), "TK2322");
        assert!(checked.diagnostics[0]
            .message
            .contains("Type 'string' is not assignable to type 'number'"));
    }
    let source = "declare function f<T,U>(seed:T,body:{keep:(x:T)=>void;introduced:U}):{seed:T;introduced:U}; const result:{seed:number;introduced:(x:string)=>void}=f(1 as const,{keep:x=>{x=2;},introduced:y=>{y=2;}});";
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    let mut interner = Interner::with_intrinsics();
    let checked = crate::check::checker::check_program(&mut interner, &parsed.program);
    assert!(checked.incomplete.is_empty());
    assert_eq!(checked.diagnostics.len(), 2, "{:?}", checked.diagnostics);
    assert!(checked
        .diagnostics
        .iter()
        .all(|diagnostic| diagnostic.code.as_str() == "TK2322"));
    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("Type '2' is not assignable to type '1'")));
    assert!(checked
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("type 'string'")));
}

#[test]
fn native_returned_context_preserves_parameter_p_at_each_callable_boundary() {
    with_expression("const source = 0;", |pass, _| {
        let wk = pass.interner.well_known();
        let binder = TypeParamId(100);
        let parameter = pass.interner.intern_type_param(binder, "T");
        let one = pass.interner.intern_literal(LiteralValue::Number(1.0));
        let inner = pass.interner.intern_function(FunctionType {
            type_params: Vec::new(),
            receiver: None,
            params: vec![ParameterType::required("value", parameter)],
            ret: parameter,
        });
        let environment = ContextualInferenceEnvironment::extend(
            None,
            &[],
            &FxHashMap::from_iter([(binder, one)]),
            &FxHashMap::from_iter([(binder, wk.number)]),
        );
        for depth in 0..=4 {
            let mut returned = inner;
            for level in 0..depth {
                returned = if level % 2 == 0 {
                    pass.interner.intern_object(ObjectType {
                        properties: vec![PropertyType::public("callback", returned)],
                        ..ObjectType::default()
                    })
                } else {
                    pass.interner.intern_tuple(vec![returned])
                };
            }
            let outer = pass.interner.intern_function(FunctionType {
                type_params: Vec::new(),
                receiver: None,
                params: Vec::new(),
                ret: returned,
            });
            pass.with_inference_environment(Some(environment.clone()), |pass| {
                let view = pass
                    .contextual_callable_input(outer)
                    .expect("outer callable");
                assert_eq!(view.ret, returned, "returned shape at depth {depth}");
                let mut nested = view.ret;
                for level in (0..depth).rev() {
                    nested = if level % 2 == 0 {
                        pass.interner
                            .store()
                            .object_type(nested)
                            .expect("object")
                            .properties[0]
                            .ty
                    } else {
                        pass.interner
                            .store()
                            .tuple_type(nested)
                            .expect("tuple")
                            .elements[0]
                    };
                }
                let nested = pass
                    .contextual_callable_input(nested)
                    .expect("returned callable");
                assert_eq!(nested.params[0].ty, one);
                assert_eq!(nested.ret, wk.number);
            });
            assert!(pass.inference_environment.is_none());
        }
    });
}

#[test]
fn native_returned_context_r_introduces_a_callable_at_a_bare_binder() {
    with_expression("const source = 0;", |pass, _| {
        let wk = pass.interner.well_known();
        let binder = TypeParamId(100);
        let parameter = pass.interner.intern_type_param(binder, "T");
        let introduced = pass.interner.intern_function(FunctionType {
            type_params: Vec::new(),
            receiver: None,
            params: vec![ParameterType::required("value", wk.number)],
            ret: wk.string,
        });
        let outer = pass.interner.intern_function(FunctionType {
            type_params: Vec::new(),
            receiver: None,
            params: Vec::new(),
            ret: parameter,
        });
        let environment = ContextualInferenceEnvironment::extend(
            None,
            &[],
            &FxHashMap::from_iter([(binder, wk.boolean)]),
            &FxHashMap::from_iter([(binder, introduced)]),
        );
        pass.with_inference_environment(Some(environment), |pass| {
            let view = pass
                .contextual_callable_input(outer)
                .expect("outer callable");
            assert_eq!(view.ret, introduced);
            let nested = pass
                .contextual_callable_input(view.ret)
                .expect("introduced callable");
            assert_eq!(nested.params[0].ty, wk.number);
            assert_eq!(nested.ret, wk.string);
        });
        assert!(pass.inference_environment.is_none());
    });
}

#[test]
fn native_parameter_and_return_views_have_independent_authority() {
    with_expression("const source = 0;", |pass, _| {
        let wk = pass.interner.well_known();
        let binder = TypeParamId(100);
        let parameter = pass.interner.intern_type_param(binder, "T");
        let signature = pass.interner.intern_function(FunctionType {
            type_params: Vec::new(),
            receiver: None,
            params: vec![ParameterType::required("value", parameter)],
            ret: parameter,
        });
        let ordinary = FxHashMap::from_iter([(binder, wk.string)]);
        for result in [None, Some(wk.number), Some(wk.unknown)] {
            let expected = result.map(|value| (binder, value)).into_iter().collect();
            let environment =
                ContextualInferenceEnvironment::extend(None, &[], &ordinary, &expected);
            pass.with_inference_environment(Some(environment), |pass| {
                let view = pass
                    .contextual_callable_input(signature)
                    .expect("callable view");
                assert_eq!(view.params[0].ty, wk.string);
                assert_eq!(view.ret, result.unwrap_or(parameter));
            });
            assert!(pass.inference_environment.is_none());
        }
    });
}

#[test]
fn native_independent_structural_value_is_not_lost_at_a_bare_binder_target() {
    with_expression("const value = [{ item: 1 }];", |pass, expression| {
        let raw = pass
            .infer_expr(pass.binder.module, expression)
            .expect("raw array")
            .0;
        let element = pass
            .interner
            .store()
            .array_type(raw)
            .expect("array")
            .element;
        let binder = TypeParamId(100);
        let parameter = pass.interner.intern_type_param(binder, "T");
        let target = pass.interner.intern_array(parameter);
        let parameters = [GenericTypeParam {
            id: binder,
            constraint: None,
            default: None,
        }];
        let mut preparation = infer::CallInferencePreparation::default();
        preparation.deferred_arguments.insert(0);
        pass.summarize_inference_shell(
            pass.binder.module,
            expression,
            ShellInferenceInput {
                raw: Some(raw),
                target,
                root: target,
                index: 0,
            },
            &parameters,
            &mut preparation,
        );
        assert_eq!(preparation.independent_inputs.len(), 1);
        assert_eq!(preparation.independent_inputs[0].source, element);
        let result = pass.infer_partial_signature_type_arguments_with_native_demands(
            infer::SignatureInferenceRequest {
                purpose: infer::SignatureInferencePurpose::ContextualPreview {
                    retained_return: parameter,
                    preparation: &preparation,
                },
                type_params: &parameters,
                params: &[ParameterType::required("items", target)],
                args: &[raw],
                fresh_args: &[true],
                optional_undefined_fallback: &[false],
                receiver: None,
            },
        );
        let DemandOutcome::Ready(result) = result else {
            panic!("ready preview");
        };
        assert_eq!(result.arguments, vec![(binder, element)]);
    });
}

#[test]
fn native_partial_headers_are_annotation_evidence_not_ready_callback_upper_bounds() {
    for (target, callback) in [
        ("(x:T,y:U)=>void", "flag ? (x,y)=>{} : (x:string,y)=>{}"),
        ("(x:T,y:U)=>void", "flag ? (x,y)=>{} : (x,y:number)=>{}"),
        (
            "(x:T,y:U)=>void",
            "flag ? (x,y:number)=>{} : (x:string,y)=>{}",
        ),
        ("{run:(x:T,y:U)=>void}", "{run:function(x:string,y){}}"),
    ] {
        let source = format!("declare const flag:boolean; declare function f<T,U>(a:T,b:U,cb:{target}):[T,U]; const value=f(\"read\",1,{callback}); const x:\"read\"=value[0]; const y:1=value[1];");
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, &source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty());
        let mut interner = Interner::with_intrinsics();
        let checked = crate::check::checker::check_program(&mut interner, &parsed.program);
        assert!(checked.incomplete.is_empty());
        assert_eq!(
            checked.diagnostics.len(),
            2,
            "{source}: {:?}",
            checked.diagnostics
        );
        assert!(checked
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code.as_str() == "TK2322"));
    }
}

#[test]
fn native_expected_result_prefers_outer_r_then_p_without_ordinary_exclusion() {
    with_expression("const source = 0;", |pass, _| {
        let wk = pass.interner.well_known();
        let outer = TypeParamId(100);
        let outer_type = pass.interner.intern_type_param(outer, "Outer");
        let inner = TypeParamId(101);
        let inner_type = pass.interner.intern_type_param(inner, "Inner");
        let parameters = [GenericTypeParam {
            id: inner,
            constraint: None,
            default: None,
        }];
        for result in [None, Some(wk.number), Some(wk.unknown)] {
            let ordinary = FxHashMap::from_iter([(outer, wk.string), (inner, wk.boolean)]);
            let expected = result.map(|value| (outer, value)).into_iter().collect();
            let environment =
                ContextualInferenceEnvironment::extend(None, &[], &ordinary, &expected);
            pass.with_inference_environment(Some(environment), |pass| {
                let map =
                    pass.prepare_expected_result_map(&parameters, inner_type, Some(outer_type));
                let Ok(map) = map else {
                    panic!("expected-result inference");
                };
                assert_eq!(map.get(&inner), Some(&result.unwrap_or(wk.string)));
                let absent = pass.prepare_expected_result_map(&parameters, inner_type, None);
                assert!(matches!(absent, Ok(map) if map.is_empty()));
            });
        }
    });
}

#[test]
fn native_same_callee_shadows_outer_result_bindings_even_when_inner_r_is_absent() {
    with_expression("const source = 0;", |pass, _| {
        let wk = pass.interner.well_known();
        let binder = TypeParamId(100);
        let unrelated = TypeParamId(101);
        let ordinary = FxHashMap::from_iter([(binder, wk.string)]);
        let expected = FxHashMap::from_iter([(binder, wk.number), (unrelated, wk.boolean)]);
        let outer = ContextualInferenceEnvironment::extend(None, &[], &ordinary, &expected);
        let parameters = [GenericTypeParam {
            id: binder,
            constraint: None,
            default: None,
        }];
        let inner = ContextualInferenceEnvironment::extend(
            Some(&outer),
            &parameters,
            &ordinary,
            &FxHashMap::default(),
        );
        assert_eq!(inner.ordinary, vec![(binder, wk.string)]);
        assert_eq!(inner.expected_result, vec![(unrelated, wk.boolean)]);
        assert_eq!(
            outer.expected_result,
            vec![(binder, wk.number), (unrelated, wk.boolean)]
        );
        let source = "declare function call<T>(value:T,cb:()=>T):T; declare function id<T>(value:T):T; const outer:1=call(1 as const,()=>{let inner=call(\"read\" as const,()=>id(\"read\"));const exact:\"read\"=inner;inner=\"write\";return 1 as const;});";
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        let mut interner = Interner::with_intrinsics();
        let checked = crate::check::checker::check_program(&mut interner, &parsed.program);
        assert!(checked.incomplete.is_empty());
        assert_eq!(checked.diagnostics.len(), 1);
        assert!(checked.diagnostics[0].message.contains("\"write\""));
    });
}

#[test]
fn native_expected_result_failure_drops_all_bindings_and_query_writes() {
    with_expression("const source = 0;", |pass, _| {
        let wk = pass.interner.well_known();
        let first = TypeParamId(100);
        let second = TypeParamId(101);
        let first_type = pass.interner.intern_type_param(first, "T");
        let second_type = pass.interner.intern_type_param(second, "U");
        let unavailable = pass
            .interner
            .intern_class_instance(ClassId(99_130), Vec::new());
        let parameters = [
            GenericTypeParam {
                id: first,
                constraint: None,
                default: None,
            },
            GenericTypeParam {
                id: second,
                constraint: Some(unavailable),
                default: None,
            },
        ];
        let target = pass.interner.intern_tuple(vec![first_type, second_type]);
        let expected = pass.interner.intern_tuple(vec![wk.string, wk.number]);
        let before = pass.semantic_queries.durable_lengths();
        for _ in 0..2 {
            let result = pass.prepare_expected_result_map(&parameters, target, Some(expected));
            assert!(matches!(result, Ok(map) if map.is_empty()));
            assert_eq!(pass.semantic_queries.durable_lengths(), before);
            assert!(pass.inference_environment.is_none());
        }
    });
}

#[test]
fn native_memo_dependencies_invalidate_without_replaying_contextual_parameter_writes() {
    with_expression(
        "declare let external: string; const source = value => external;",
        |pass, expression| {
            let wk = pass.interner.well_known();
            let binder = TypeParamId(100);
            let parameter = pass.interner.intern_type_param(binder, "T");
            let target = pass.interner.intern_function(FunctionType {
                type_params: Vec::new(),
                receiver: None,
                params: vec![ParameterType::required("value", parameter)],
                ret: wk.void,
            });
            let external = pass
                .binder
                .resolve_value(pass.binder.module, "external")
                .and_then(|symbol| pass.binder.symbols.get(symbol))
                .and_then(|symbol| symbol.value)
                .expect("external slot");
            let environment = ContextualInferenceEnvironment::extend(
                None,
                &[],
                &FxHashMap::from_iter([(binder, wk.string)]),
                &FxHashMap::default(),
            );
            let mut events = crate::check::checker::events::EventStore::default();
            let owner = events.reserve_event(
                crate::source::ModuleOrdinal::new(0),
                expression.span().start,
            );
            pass.effect_stack.push(CheckerEffects::new(owner.primary));
            pass.argument_walk_depth = 1;
            DeclTypes::open_log();
            pass.with_inference_environment(Some(environment), |pass| {
                let raw = (wk.unknown, Span::from_oxc(expression.span()));
                pass.decl_types.set(external, wk.string);
                let cold = pass.infer_contextual_source_after_walked_reporting(
                    pass.binder.module,
                    expression,
                    target,
                    raw,
                    true,
                    false,
                );
                assert_eq!(cold.1, ContextualRewalk::Rewalked);
                reset_call_measure();
                let mark = DeclTypes::log_mark();
                let warm = pass.infer_contextual_source_after_walked_reporting(
                    pass.binder.module,
                    expression,
                    target,
                    raw,
                    true,
                    false,
                );
                assert_eq!(warm, cold);
                assert_eq!(call_measure().contextual_memo_hits, 1);
                assert_eq!(
                    DeclTypes::dependencies_since(mark),
                    vec![(external, Some(wk.string))]
                );
                assert!(DeclTypes::net_writes_since(mark).is_empty());
                pass.decl_types.set(external, wk.number);
                reset_call_measure();
                let changed = pass.infer_contextual_source_after_walked_reporting(
                    pass.binder.module,
                    expression,
                    target,
                    raw,
                    true,
                    false,
                );
                assert_eq!(changed.1, ContextualRewalk::Rewalked);
                assert_eq!(call_measure().contextual_memo_hits, 0);
                assert_eq!(
                    pass.interner
                        .store()
                        .function_type(changed.0 .0)
                        .expect("callback")
                        .ret,
                    wk.number
                );
            });
            DeclTypes::close_log();
            assert!(pass.inference_environment.is_none());
        },
    );
}

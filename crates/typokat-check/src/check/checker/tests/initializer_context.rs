use super::*;
use crate::check::checker::type_groups::{PublishedTypeEnvironment, TypeEnvironmentState};
use crate::check::query::{query_demand_measure, reset_query_demand_measure};
use crate::types::repr::{ConditionalType, EnumId};
use oxc_allocator::Allocator;
use oxc_ast::ast::Statement;
use oxc_parser::Parser;
use oxc_span::SourceType;

fn with_pass(test: impl FnOnce(&mut Pass<'_, '_>)) {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, "", SourceType::ts()).parse();
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
    test(&mut pass);
}

fn keyof_context(pass: &mut Pass<'_, '_>) -> (TypeId, TypeId) {
    let wk = pass.interner.well_known();
    let object = pass.interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::public("alpha", wk.number),
            PropertyType::public("beta", wk.string),
        ],
        ..Default::default()
    });
    let context = pass.interner.intern_keyof(object);
    let alpha = pass
        .interner
        .intern_literal(LiteralValue::String("alpha".into()));
    let beta = pass
        .interner
        .intern_literal(LiteralValue::String("beta".into()));
    let effective = pass.interner.union(vec![alpha, beta]);
    (context, effective)
}

#[test]
fn prepared_context_views_keep_symbolic_identity_and_real_unknown() {
    with_pass(|pass| {
        let wk = pass.interner.well_known();
        let (context, effective) = keyof_context(pass);
        let prepared = pass.prepare_initializer_context(context).unwrap();
        assert_eq!(prepared.original_target, context);
        assert_eq!(prepared.publication_context, effective);
        assert_eq!(prepared.shaping_context, effective);
        let parameter = pass.interner.intern_type_param(TypeParamId(1), "T");
        assert!(pass
            .interner
            .set_type_param_constraint(TypeParamId(1), context));
        let prepared = pass.prepare_initializer_context(parameter).unwrap();
        assert_eq!(prepared.publication_context, parameter);
        assert_eq!(prepared.shaping_context, effective);
        for members in [vec![parameter, wk.number], vec![wk.number, parameter]] {
            let union = pass.interner.union(members);
            let prepared = pass.prepare_initializer_context(union).unwrap();
            assert_eq!(prepared.publication_context, union);
            assert_eq!(
                prepared.shaping_context,
                pass.interner.union(vec![effective, wk.number])
            );
        }
        let unknown = pass.prepare_initializer_context(wk.unknown).unwrap();
        assert_eq!(unknown.publication_context, wk.unknown);
        assert_eq!(unknown.shaping_context, wk.unknown);
        assert_eq!(
            pass.prepare_initializer_context(wk.error),
            Err(ContextPreparationFailure::Unavailable(
                ContextUnavailable::InvalidTarget
            ))
        );
        let symbolic = pass.interner.intern_type_param(TypeParamId(2), "U");
        let symbolic_keyof = pass.interner.intern_keyof(symbolic);
        let prepared = pass.prepare_initializer_context(symbolic_keyof).unwrap();
        assert_eq!(prepared.publication_context, symbolic_keyof);
    });
}

#[test]
fn late_unavailable_and_exhausted_alternatives_rollback_successful_query_writes() {
    for unavailable in [true, false] {
        with_pass(|pass| {
            let (keys, effective) = keyof_context(pass);
            let wk = pass.interner.well_known();
            let successful = pass.interner.intern_conditional(ConditionalType {
                check: wk.string,
                extends_ty: wk.string,
                true_branch: keys,
                false_branch: wk.never,
                infer_count: 0,
                distributive: false,
                poisoned: false,
            });
            let failing = if unavailable {
                let cycle = pass.interner.intern_type_param(TypeParamId(9), "Cycle");
                assert!(pass
                    .interner
                    .set_type_param_constraint(TypeParamId(9), cycle));
                cycle
            } else {
                pass.interner.reserve_enum(EnumId(9), "Pending")
            };
            let union = pass.interner.union(vec![successful, failing]);
            assert_eq!(
                pass.interner.store().union_members(union).unwrap()[0],
                successful
            );
            let before = pass.semantic_queries.durable_lengths();
            reset_query_demand_measure();
            let result = pass.prepare_initializer_context(union);
            if unavailable {
                assert_eq!(
                    result,
                    Err(ContextPreparationFailure::Unavailable(
                        ContextUnavailable::ConstraintCycle
                    ))
                );
            } else {
                assert!(matches!(
                    result,
                    Err(ContextPreparationFailure::Exhausted(
                        Exhaustion::EvaluationInvalidNode { .. }
                    ))
                ));
            }
            assert!(
                query_demand_measure().evaluation_changed_returns > 0,
                "the first alternative must have performed real work: {:?}",
                query_demand_measure()
            );
            assert_eq!(
                pass.semantic_queries.durable_lengths(),
                before,
                "neither failure may promote the earlier alternative's writes"
            );
            reset_query_demand_measure();
            assert_eq!(
                pass.prepare_initializer_context(successful)
                    .unwrap()
                    .publication_context,
                effective
            );
            let cold = query_demand_measure();
            assert_eq!(
                cold.durable_evaluation_hits, 0,
                "failed preparation must not seed a later query"
            );
            assert!(cold.evaluation_changed_returns > 0);
            assert_ne!(
                pass.semantic_queries.durable_lengths(),
                before,
                "the successful alternative alone must promote query writes"
            );
        });
    }
}

#[test]
fn transparent_values_and_return_slot_prepare_once() {
    for expression in [
        "\"alpha\"",
        "(((\"alpha\")))",
        "true ? \"alpha\" : \"beta\"",
        "true && \"alpha\"",
        "false || \"alpha\"",
        "null ?? \"alpha\"",
    ] {
        with_pass(|pass| {
            let (context, effective) = keyof_context(pass);
            let allocator = Allocator::default();
            let source = format!("const value = {expression};");
            let parsed = Parser::new(&allocator, &source, SourceType::ts()).parse();
            assert!(parsed.diagnostics.is_empty());
            let Statement::VariableDeclaration(declaration) = &parsed.program.body[0] else {
                panic!("variable")
            };
            let expression = declaration.declarations[0].init.as_ref().unwrap();
            let slot = InitializerContextSlot::new(Some(context));
            assert_eq!(slot.state.get(), InitializerContextState::Unprepared);
            reset_query_demand_measure();
            let value = pass
                .infer_initializer_with_context_slot(
                    pass.binder.module,
                    expression,
                    &slot,
                    ContextualLiteralProvenance::Existing,
                )
                .unwrap();
            assert_eq!(value.publication_context(), Some(effective));
            let first = query_demand_measure();
            assert_eq!(first.root_calls, 3, "{source}: {first:?}");
            pass.infer_initializer_with_context_slot(
                pass.binder.module,
                expression,
                &slot,
                ContextualLiteralProvenance::Existing,
            )
            .unwrap();
            assert_eq!(
                query_demand_measure().root_calls,
                first.root_calls,
                "body-local reuse"
            );
        });
    }
}

#[test]
fn contextual_memo_keeps_original_symbolic_and_this_identities() {
    with_pass(|pass| {
        let wk = pass.interner.well_known();
        let object = pass.interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("value", wk.string)],
            ..Default::default()
        });
        let this = pass.interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("thisValue", wk.number)],
            ..Default::default()
        });
        let marker = pass
            .interner
            .intern_instantiation(wk.this_type, vec![(TypeParamId(0), this)]);
        let intersection = pass.interner.intersection(vec![object, marker]);
        let prepared = pass.prepare_initializer_context(intersection).unwrap();
        assert_eq!(prepared.contextual_this, Some(this));
        assert!(pass
            .interner
            .store()
            .object_type(prepared.shaping_context)
            .is_some());
        let first = pass.interner.intern_type_param(TypeParamId(1), "T");
        let second = pass.interner.intern_type_param(TypeParamId(2), "U");
        assert!(pass
            .interner
            .set_type_param_constraint(TypeParamId(1), intersection));
        assert!(pass
            .interner
            .set_type_param_constraint(TypeParamId(2), intersection));
        let first = pass.prepare_initializer_context(first).unwrap();
        let second = pass.prepare_initializer_context(second).unwrap();
        assert_eq!(first.shaping_context, second.shaping_context);
        assert_eq!(first.contextual_this, Some(this));
        pass.argument_walk_depth = 1;
        let raw = (object, Span::new(0, 1));
        let key = pass
            .contextual_walk_key(pass.binder.module, raw, first, false)
            .unwrap();
        let other = pass
            .contextual_walk_key(pass.binder.module, raw, second, false)
            .unwrap();
        assert!(key != other);
        pass.memoize_contextual_walk(
            key.clone(),
            DeclTypes::log_mark(),
            (CheckedExpr::opaque(raw), ContextualRewalk::Rewalked),
        );
        assert_eq!(
            pass.memoized_contextual_walk(&key)
                .map(|(value, status)| (value.tuple(), status)),
            Some((raw, ContextualRewalk::Rewalked))
        );
        assert!(pass.memoized_contextual_walk(&other).is_none());
    });
}

#[test]
fn repeated_value_returns_reuse_preparation_at_the_body_owner() {
    let mut counts = Vec::new();
    for returns in [1, 3] {
        let source = format!("interface Keys {{ alpha: number; beta: string }} const callback = (() => {{ {} }}) satisfies () => keyof Keys;", "return \"alpha\";".repeat(returns));
        let count = crate::check::test_support::on_raw_check_worker(|| {
            let run = crate::frontend::run_source_frontend(&source, |program| {
                let mut interner = Interner::with_intrinsics();
                reset_query_demand_measure();
                let result = crate::check::checker::check_program(&mut interner, program);
                assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
                assert!(result.incomplete.is_empty(), "{:?}", result.incomplete);
                query_demand_measure().root_calls
            });
            run.product.unwrap()
        })
        .unwrap();
        counts.push(count);
    }
    assert_eq!(counts[0], counts[1]);
}

#[test]
fn invalid_annotation_keeps_source_errors_without_publishing_a_ready_context() {
    for source in [
        "const value: Missing = missingValue;",
        "const value: Missing = missingTest ? missingLeft : missingRight;",
        "const value: Missing = missingLeft && missingRight;",
        "const value: Missing = missingLeft || missingRight;",
        "const value: Missing = (() => missingBody);",
    ] {
        let output = crate::check::test_support::check_source(source);
        assert!(output.parse_errors.is_empty());
        assert!(
            output.incomplete.is_empty(),
            "{source}: {:?}",
            output.incomplete
        );
        let actual = output
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code.as_str(), &source[diagnostic.span.range()]))
            .collect::<Vec<_>>();
        let names = source
            .split(|ch: char| !ch.is_alphanumeric())
            .filter(|name| *name == "Missing" || name.starts_with("missing"))
            .map(|name| ("TK2304", name))
            .collect::<Vec<_>>();
        assert_eq!(actual, names, "{source}");
    }
}

#[test]
fn selected_and_rejected_contexts_keep_default_effects_and_query_rollback() {
    crate::check::test_support::on_raw_check_worker(|| {
        let source = r#"
interface Keys { alpha: number; beta: string }
declare function numeric(value: number): number;
declare function choose(tag: "key", callback: (x?: number) => number): void;
declare function choose(tag: "key", callback: (x?: number) => keyof Keys): void;
choose("key", (x = numeric("bad")) => "alpha");
"#;
        let run = crate::frontend::run_source_frontend(source, |program| {
            let mut interner = Interner::with_intrinsics();
            crate::check::checker::calls::reset_call_measure();
            let output = crate::check::checker::check_program(&mut interner, program);
            assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
            assert_eq!(
                output
                    .diagnostics
                    .iter()
                    .map(|diagnostic| (diagnostic.code.as_str(), &source[diagnostic.span.range()]))
                    .collect::<Vec<_>>(),
                [("TK2345", "\"bad\"")]
            );
            let measure = crate::check::checker::calls::call_measure();
            assert!(
                measure.candidate_matches > 0 && measure.candidate_mismatches > 0,
                "{measure:?}"
            );
            assert!(
                measure.speculative_query_forks > 0
                    && measure.speculative_query_writes_discarded > 0,
                "{measure:?}"
            );
        });
        assert!(run.parse_errors.is_empty());
        assert!(run.product.is_some());
    })
    .unwrap();
}

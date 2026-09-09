use super::*;
use crate::check::checker::type_groups::{PublishedTypeEnvironment, TypeEnvironmentState};
use crate::source::{ModuleOrdinal, SourceUnit, UnitSlot};
use crate::types::repr::{ClassId, EnumId};
use oxc_allocator::Allocator;
use oxc_ast::ast::{ClassElement, Statement};
use oxc_parser::Parser;
use oxc_span::SourceType;

fn with_validation_pass(test: impl FnOnce(&mut Pass<'_, '_>, &Expression<'_>)) {
    let allocator = Allocator::default();
    let parsed = Parser::new(
        &allocator,
        "class C { field = { key: 'alpha' }; }",
        SourceType::ts(),
    )
    .parse();
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
            ModuleOrdinal::new(0),
            UnitSlot::new(0),
            None,
        );
    let member = reporting.lexical_events.classes()[0].members[0];
    let ticket = reporting
        .lexical_events
        .member(member)
        .unwrap()
        .tickets
        .immediate;
    pass.current_class = Some(ClassId(0));
    let Statement::ClassDeclaration(class) = &parsed.program.body[0] else {
        panic!("class")
    };
    let ClassElement::PropertyDefinition(property) = &class.body.body[0] else {
        panic!("field")
    };
    let expression = property.value.as_ref().unwrap();
    let wk = pass.interner.well_known();
    let retained = pass.interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("key", wk.string)],
        ..Default::default()
    });
    let mut rows = InitializerValidationRows::default();
    rows.insert(
        Span::from_oxc(expression.span()),
        InitializerValidationBoundary::CompletedContainer,
        retained,
    );
    pass.class_initializer_validation.insert(
        member,
        InitializerValidationSources {
            source: pass.current_source,
            class: ClassId(0),
            member,
            rows,
        },
    );
    pass.active_initializer_validation = Some(member);
    pass.with_ticket_effects(ticket, |pass| test(pass, expression));
}

#[test]
fn retained_validation_distinguishes_failed_preparation_and_partial_rewalk() {
    with_validation_pass(|pass, expression| {
        let wk = pass.interner.well_known();
        let raw = (wk.unknown, Span::from_oxc(expression.span()));
        let pending = pass.interner.reserve_enum(EnumId(0), "Pending");
        let before = pass.semantic_queries.durable_lengths();
        let (_, status) = pass.infer_contextual_source_after_walked_reporting(
            pass.binder.module,
            expression,
            pending,
            raw,
            false,
            true,
        );
        assert_eq!(status, ContextualRewalk::Failed);
        assert!(!status.source_ready());
        assert_eq!(pass.semantic_queries.durable_lengths(), before);
        let (_, status) = pass.infer_contextual_source_after_walked_reporting(
            pass.binder.module,
            expression,
            wk.unknown,
            raw,
            false,
            true,
        );
        assert_eq!(status, ContextualRewalk::KeptRaw);
        assert!(status.source_ready());
        let cyclic = pass.interner.intern_type_param(TypeParamId(12), "Cyclic");
        assert!(pass
            .interner
            .set_type_param_constraint(TypeParamId(12), cyclic));
        let target = pass.interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("key", cyclic)],
            ..Default::default()
        });
        let ((source, _), status) = pass.infer_contextual_source_after_walked_reporting(
            pass.binder.module,
            expression,
            target,
            raw,
            false,
            true,
        );
        assert_eq!(status, ContextualRewalk::RewalkedFailed);
        assert!(status.was_rewalked());
        assert!(!status.source_ready());
        assert!(pass
            .interner
            .store()
            .object_type(source)
            .unwrap()
            .properties
            .is_empty());
        assert!(
            !pass.initializer_validation_failed,
            "attempt restores parent status"
        );
    });
}

#[test]
fn retained_validation_uses_full_occurrences_and_exact_owner_scope() {
    with_validation_pass(|pass, expression| {
        let occurrence = Span::from_oxc(expression.span());
        let wk = pass.interner.well_known();
        let retained = pass.initializer_validation_source(
            occurrence,
            InitializerValidationBoundary::CompletedContainer,
            wk.unknown,
        );
        assert_ne!(retained, wk.unknown);
        for other in [
            Span::new(occurrence.start, occurrence.end - 1),
            Span::new(occurrence.start + 1, occurrence.end),
        ] {
            assert_eq!(
                pass.initializer_validation_source(
                    other,
                    InitializerValidationBoundary::CompletedContainer,
                    wk.unknown
                ),
                wk.unknown
            );
        }
        assert_eq!(
            pass.initializer_validation_source(
                occurrence,
                InitializerValidationBoundary::SatisfiesOperand,
                wk.unknown
            ),
            wk.unknown
        );
        assert_eq!(
            pass.initializer_validation_source(
                occurrence,
                InitializerValidationBoundary::CompletedContainer,
                wk.error
            ),
            wk.error
        );
        pass.current_class = Some(ClassId(1));
        assert_eq!(
            pass.initializer_validation_source(
                occurrence,
                InitializerValidationBoundary::CompletedContainer,
                wk.unknown
            ),
            wk.unknown
        );
        pass.current_class = Some(ClassId(0));
        let source = pass.current_source;
        pass.current_source = SourceUnit::User {
            module_ordinal: ModuleOrdinal::new(1),
            unit_slot: UnitSlot::new(1),
        };
        assert_eq!(
            pass.initializer_validation_source(
                occurrence,
                InitializerValidationBoundary::CompletedContainer,
                wk.unknown
            ),
            wk.unknown
        );
        pass.current_source = source;
        for _ in 0..3 {
            assert_eq!(
                pass.initializer_validation_source(
                    occurrence,
                    InitializerValidationBoundary::CompletedContainer,
                    wk.unknown
                ),
                retained
            );
        }
    });
}

#[test]
fn retained_validation_scope_refuses_effect_discarding_contextual_memo() {
    with_validation_pass(|pass, expression| {
        let wk = pass.interner.well_known();
        pass.argument_walk_depth = 1;
        assert!(pass.current_body_this_environment.is_none());
        let context = pass.prepare_initializer_context(wk.unknown).unwrap();
        let raw = (wk.unknown, Span::from_oxc(expression.span()));
        assert!(pass
            .contextual_walk_key(pass.binder.module, raw, context, true)
            .is_none());
        let validation = pass.active_initializer_validation.take();
        assert!(pass
            .contextual_walk_key(pass.binder.module, raw, context, true)
            .is_some());
        pass.active_initializer_validation = validation;
        assert!(pass
            .contextual_walk_key(pass.binder.module, raw, context, true)
            .is_none());
    });
}

#[test]
fn retained_validation_and_unsupported_default_keep_exact_event_owners() {
    use crate::check::checker::events::EventKey;
    use crate::check::checker::lexical_events::LexicalOwnerPhase;
    use crate::check::checker::reporting_record::CheckerRecord;
    use crate::source::SourceOrdinal;
    use std::cell::RefCell;
    crate::check::test_support::on_raw_check_worker(|| {
        let source = "interface Keys { alpha: number; beta: string }\nclass C {\n member = { key: 'alpha' } satisfies { key: keyof Keys };\n callback = ((value: number = \"bad\") => value) satisfies (value?: number) => number;\n}";
        let key = RefCell::new(Vec::new());
        let run = crate::frontend::run_project_frontend(vec![crate::frontend::FileInput {
            name: "/validation/main.ts".into(), source: source.into(),
        }], |interner, units| {
            crate::check::checker::check_project_programs_inner(interner, units, None, None,
                |_, reservations, _| {
                    let module = ModuleOrdinal::new(0);
                    let start = u32::try_from(source.find("member =").unwrap()).unwrap();
                    let ticket = reservations.owner_at(SourceOrdinal::User(module), start, LexicalOwnerPhase::Body).unwrap().ticket;
                    key.borrow_mut().push(EventKey { module_ordinal: module, source_start: start, event_ordinal: ticket.event.index(), record_ordinal: ticket.record_ordinal });
                    let start = u32::try_from(source.find("\"bad\"").unwrap()).unwrap();
                    let ticket = reservations.initializer_owner_at(SourceUnit::User { module_ordinal: module, unit_slot: UnitSlot::new(0) }, start).unwrap();
                    key.borrow_mut().push(EventKey { module_ordinal: module, source_start: start, event_ordinal: ticket.event.index(), record_ordinal: ticket.record_ordinal });
                }, |_, _, _, _, _, _, _| {}, |records| {
                    let diagnostics: Vec<_> = records.iter().filter_map(|(key, record)| {
                        let CheckerRecord::Diagnostic(diagnostic) = record else { return None; };
                        Some((*key, diagnostic.code.as_str(), &source[diagnostic.span.range()]))
                    }).collect();
                    assert_eq!(diagnostics, [(key.borrow()[0], "TK1360", "key"), (key.borrow()[1], "TK2322", "value")]);
                    assert_eq!(records.len(), 3, "{records:?}");
                },
            ).unwrap()
        });
        assert!(run.parse_errors.iter().all(Vec::is_empty));
        assert_eq!(run.product[0].incomplete.len(), 1);
        assert_eq!(run.product[0].diagnostics.len(), 2);
    }).unwrap();
}

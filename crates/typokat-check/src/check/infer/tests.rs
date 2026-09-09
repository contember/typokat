use super::*;

#[test]
fn native_boundaries_equal_upper_inputs_preserve_ordinary_mode_and_occurrence_order() {
    let mut interner = Interner::with_intrinsics();
    let binder = TypeParamId(100);
    let parameter = interner.intern_type_param(binder, "T");
    let callback = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required("value", parameter)],
        ret: interner.well_known().void,
    });
    let ordinary = interner.intern_ordinary_literal(LiteralValue::String("read".into()));
    let regular = interner.regularize_literal_type(ordinary);
    for source in [ordinary, regular] {
        for bound in [regular, interner.well_known().string] {
            for reverse in [false, true] {
                for constraint in [None, Some(interner.well_known().string)] {
                    let mut candidates = vec![
                        CallSiteCandidate {
                            ty: source,
                            original_target: parameter,
                            source: CallSiteSource::Argument {
                                index: 0,
                                occurrence: 0,
                            },
                            fresh: false,
                            optional_undefined_fallback: false,
                        },
                        CallSiteCandidate {
                            ty: bound,
                            original_target: callback,
                            source: CallSiteSource::Argument {
                                index: 1,
                                occurrence: 0,
                            },
                            fresh: true,
                            optional_undefined_fallback: false,
                        },
                    ];
                    if reverse {
                        candidates.reverse();
                    }
                    let before = candidates.clone();
                    let mut preparation = CallInferencePreparation::default();
                    preparation.fixed.insert(binder);
                    preparation.top_level.insert(binder, false);
                    preparation.upper_inputs.insert((1, binder));
                    let mut queries = SemanticQueryState::default();
                    let published = PublishedClasses::empty();
                    let compatible = is_assignable_type_attempt(
                        &mut interner,
                        &mut 101,
                        &published,
                        &mut queries,
                        source,
                        bound,
                    );
                    assert!(matches!(
                        compatible,
                        SemanticQueryAttempt::Complete(DemandOutcome::Ready(true))
                    ));
                    let result = fix_call_site_candidates_for_param_attempt(
                        &mut interner,
                        &mut 101,
                        &published,
                        &mut queries,
                        &candidates,
                        BinderFixingPolicy {
                            binder,
                            constraint,
                            preserve_unwidened: false,
                            purpose: SignatureInferencePurpose::FinalCall {
                                retained_return: parameter,
                                preparation: &preparation,
                            },
                        },
                    );
                    let expected = if constraint.is_some() {
                        regular
                    } else {
                        source
                    };
                    assert!(
                        matches!(result, SemanticQueryAttempt::Complete(DemandOutcome::Ready(Some(value))) if value == expected)
                    );
                    assert_eq!(candidates, before);
                    assert_eq!(candidates.len(), 2);
                }
            }
        }
    }
}

#[test]
fn native_modes_freeze_before_rest_echo_without_removing_occurrences() {
    let mut interner = Interner::with_intrinsics();
    let binder = TypeParamId(100);
    let parameter = interner.intern_type_param(binder, "T");
    let array = interner.intern_array(parameter);
    let callback = interner.intern_function(crate::types::repr::FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::rest("values", array)],
        ret: interner.well_known().void,
    });
    let ordinary =
        interner.intern_ordinary_literal(crate::types::repr::LiteralValue::String("read".into()));
    let regular = interner.regularize_literal_type(ordinary);
    let published = PublishedClasses::empty();
    for (source, expected) in [(ordinary, interner.well_known().string), (regular, regular)] {
        let candidates = vec![
            CallSiteCandidate {
                ty: source,
                original_target: parameter,
                source: CallSiteSource::Argument {
                    index: 0,
                    occurrence: 0,
                },
                fresh: false,
                optional_undefined_fallback: false,
            },
            CallSiteCandidate {
                ty: interner.well_known().never,
                original_target: callback,
                source: CallSiteSource::Argument {
                    index: 1,
                    occurrence: 0,
                },
                fresh: true,
                optional_undefined_fallback: false,
            },
        ];
        let before = candidates.clone();
        let mut preparation = CallInferencePreparation::default();
        preparation.fixed.insert(binder);
        preparation.top_level.insert(binder, true);
        assert!(!type_parameter_at_top_level(
            interner.store(),
            callback,
            binder
        ));
        let result = fix_call_site_candidates_for_param_attempt(
            &mut interner,
            &mut 101,
            &published,
            &mut SemanticQueryState::default(),
            &candidates,
            BinderFixingPolicy {
                binder,
                constraint: None,
                preserve_unwidened: false,
                purpose: SignatureInferencePurpose::FinalCall {
                    retained_return: parameter,
                    preparation: &preparation,
                },
            },
        );
        let SemanticQueryAttempt::Complete(DemandOutcome::Ready(result)) = result else {
            panic!("ready selection expected");
        };
        assert_eq!(result, Some(expected));
        assert_eq!(candidates, before);
        assert_eq!(candidates.len(), 2);
    }
}

#[test]
fn native_boundaries_conflicting_upper_inputs_still_use_baseline_selection() {
    let mut interner = Interner::with_intrinsics();
    let binder = TypeParamId(100);
    let parameter = interner.intern_type_param(binder, "T");
    let read = interner.intern_ordinary_literal(LiteralValue::String("read".into()));
    let regular_read = interner.regularize_literal_type(read);
    let write = interner.intern_literal(LiteralValue::String("write".into()));
    let conflict = interner.union(vec![read, write]);
    for bounds in [[regular_read, write], [write, regular_read]] {
        let candidates: Vec<_> = [read, bounds[0], bounds[1]]
            .into_iter()
            .enumerate()
            .map(|(index, ty)| CallSiteCandidate {
                ty,
                original_target: parameter,
                source: CallSiteSource::Argument {
                    index,
                    occurrence: 0,
                },
                fresh: false,
                optional_undefined_fallback: false,
            })
            .collect();
        let before = candidates.clone();
        let mut preparation = CallInferencePreparation::default();
        preparation.fixed.insert(binder);
        preparation.top_level.insert(binder, false);
        preparation.upper_inputs.extend([(1, binder), (2, binder)]);
        let mut queries = SemanticQueryState::default();
        let published = PublishedClasses::empty();
        let incompatible = is_assignable_type_attempt(
            &mut interner,
            &mut 101,
            &published,
            &mut queries,
            read,
            write,
        );
        assert!(matches!(
            incompatible,
            SemanticQueryAttempt::Complete(DemandOutcome::Ready(false))
        ));
        let result = fix_call_site_candidates_for_param_attempt(
            &mut interner,
            &mut 101,
            &published,
            &mut queries,
            &candidates,
            BinderFixingPolicy {
                binder,
                constraint: None,
                preserve_unwidened: true,
                purpose: SignatureInferencePurpose::FinalCall {
                    retained_return: parameter,
                    preparation: &preparation,
                },
            },
        );
        assert!(
            matches!(result, SemanticQueryAttempt::Complete(DemandOutcome::Ready(Some(value))) if value == conflict)
        );
        for target in [regular_read, write] {
            let rejected = is_assignable_type_attempt(
                &mut interner,
                &mut 101,
                &published,
                &mut queries,
                conflict,
                target,
            );
            assert!(matches!(
                rejected,
                SemanticQueryAttempt::Complete(DemandOutcome::Ready(false))
            ));
        }
        assert_eq!(candidates, before);
        assert_eq!(candidates.len(), 3);
    }
}

#[test]
fn native_duplicate_modes_are_prepared_before_union_in_both_orders() {
    let mut interner = Interner::with_intrinsics();
    let binder = TypeParamId(100);
    let parameter = interner.intern_type_param(binder, "T");
    let retained_return = interner.intern_array(parameter);
    let ordinary =
        interner.intern_ordinary_literal(crate::types::repr::LiteralValue::String("read".into()));
    let regular = interner.regularize_literal_type(ordinary);
    for sources in [[ordinary, regular], [regular, ordinary]] {
        let candidates: Vec<_> = sources
            .into_iter()
            .enumerate()
            .map(|(index, ty)| CallSiteCandidate {
                ty,
                original_target: parameter,
                source: CallSiteSource::Argument {
                    index,
                    occurrence: 0,
                },
                fresh: false,
                optional_undefined_fallback: false,
            })
            .collect();
        let preparation = CallInferencePreparation::default();
        let result = fix_call_site_candidates_for_param_attempt(
            &mut interner,
            &mut 101,
            &PublishedClasses::empty(),
            &mut SemanticQueryState::default(),
            &candidates,
            BinderFixingPolicy {
                binder,
                constraint: None,
                preserve_unwidened: false,
                purpose: SignatureInferencePurpose::FinalCall {
                    retained_return,
                    preparation: &preparation,
                },
            },
        );
        let SemanticQueryAttempt::Complete(DemandOutcome::Ready(result)) = result else {
            panic!("ready selection expected");
        };
        assert_eq!(result, Some(interner.well_known().string));
        assert_eq!(
            candidates
                .iter()
                .map(|candidate| candidate.ty)
                .collect::<Vec<_>>(),
            sources
        );
    }
}
use crate::types::repr::{
    DeclaredRecipeNode, EnumId, EnumMemberFact, EnumMemberId, EnumMemberType, EnumNumber, EnumType,
    EnumValue, FunctionType, LiteralValue, ObjectType, ParameterType, PropertyType, TupleRestType,
    TupleType, TypeParamId, WellKnownSymbol,
};
use std::time::Instant;

fn prop(name: &str, ty: TypeId) -> PropertyType {
    PropertyType::public(name, ty)
}

fn intern_enum_family(
    interner: &mut Interner,
    enum_id: EnumId,
    name: &str,
    values: Vec<EnumValue>,
    numeric_open: bool,
) -> (TypeId, Vec<TypeId>) {
    let facts = values
        .iter()
        .enumerate()
        .map(|(index, value)| EnumMemberFact {
            id: EnumMemberId(u32::try_from(index).expect("test enum member count fits u32")),
            name: format!("M{index}"),
            value: value.clone(),
        })
        .collect();
    let whole = interner.intern_enum(EnumType {
        id: enum_id,
        name: name.to_owned(),
        members: facts,
        numeric_open,
    });
    let members = values
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            interner.intern_enum_member(EnumMemberType {
                enum_id,
                member_id: EnumMemberId(
                    u32::try_from(index).expect("test enum member count fits u32"),
                ),
                enum_name: name.to_owned(),
                member_name: format!("M{index}"),
                value,
                numeric_open,
            })
        })
        .collect();
    (whole, members)
}

#[test]
fn enum_primitive_families_preserve_whole_and_member_facts() {
    let mut interner = Interner::with_intrinsics();
    let numeric_member = interner.intern_enum_member(EnumMemberType {
        enum_id: EnumId(1),
        member_id: EnumMemberId(0),
        enum_name: "Numeric".to_owned(),
        member_name: "One".to_owned(),
        value: EnumValue::Number(EnumNumber::new(1.0).expect("finite enum constant")),
        numeric_open: false,
    });
    let string_member = interner.intern_enum_member(EnumMemberType {
        enum_id: EnumId(2),
        member_id: EnumMemberId(0),
        enum_name: "Stringy".to_owned(),
        member_name: "Ready".to_owned(),
        value: EnumValue::String("ready".to_owned()),
        numeric_open: false,
    });
    let numeric_whole = interner.intern_enum(EnumType {
        id: EnumId(1),
        name: "Numeric".to_owned(),
        members: vec![EnumMemberFact {
            id: EnumMemberId(0),
            name: "One".to_owned(),
            value: EnumValue::Number(EnumNumber::new(1.0).expect("finite enum constant")),
        }],
        numeric_open: false,
    });
    let heterogeneous = interner.intern_enum(EnumType {
        id: EnumId(3),
        name: "Mixed".to_owned(),
        members: vec![
            EnumMemberFact {
                id: EnumMemberId(0),
                name: "One".to_owned(),
                value: EnumValue::Number(EnumNumber::new(1.0).expect("finite enum constant")),
            },
            EnumMemberFact {
                id: EnumMemberId(1),
                name: "Ready".to_owned(),
                value: EnumValue::String("ready".to_owned()),
            },
        ],
        numeric_open: false,
    });
    let empty_open = interner.intern_enum(EnumType {
        id: EnumId(4),
        name: "Empty".to_owned(),
        members: Vec::new(),
        numeric_open: true,
    });

    assert!(
        primitive_families(interner.store(), numeric_member) == Some(vec![PrimitiveFamily::Number])
    );
    assert!(
        primitive_families(interner.store(), string_member) == Some(vec![PrimitiveFamily::String])
    );
    assert!(
        primitive_families(interner.store(), numeric_whole) == Some(vec![PrimitiveFamily::Number])
    );
    assert!(
        primitive_families(interner.store(), heterogeneous)
            == Some(vec![PrimitiveFamily::Number, PrimitiveFamily::String])
    );
    assert!(
        primitive_families(interner.store(), empty_open) == Some(vec![PrimitiveFamily::Number])
    );
}

#[test]
fn enum_provenance_tracks_union_identity_and_raw_mixing() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let (_, first) = intern_enum_family(
        &mut interner,
        EnumId(1),
        "First",
        vec![
            EnumValue::Number(EnumNumber::new(1.0).expect("finite enum constant")),
            EnumValue::Number(EnumNumber::new(2.0).expect("finite enum constant")),
        ],
        false,
    );
    let (_, second) = intern_enum_family(
        &mut interner,
        EnumId(2),
        "Second",
        vec![EnumValue::Number(
            EnumNumber::new(1.0).expect("finite enum constant"),
        )],
        false,
    );
    let same_enum = interner.union(vec![first[0], first[1]]);
    let cross_enum = interner.union(vec![first[0], second[0]]);
    let enum_and_raw = interner.union(vec![first[0], wk.number]);
    let raw = interner.union(vec![wk.number, wk.null]);

    assert_eq!(
        enum_provenance(interner.store(), same_enum),
        EnumProvenance::One(EnumId(1))
    );
    assert_eq!(
        enum_provenance(interner.store(), cross_enum),
        EnumProvenance::Mixed
    );
    assert_eq!(
        enum_provenance(interner.store(), enum_and_raw),
        EnumProvenance::Mixed
    );
    assert_eq!(enum_provenance(interner.store(), raw), EnumProvenance::None);
}

#[derive(Default)]
struct TestInferenceNormalization {
    replacements: FxHashMap<TypeId, TypeId>,
    demand: Option<TypeId>,
}

impl RelationNormalization for TestInferenceNormalization {
    fn normalize(&self, ty: TypeId) -> Result<TypeId, Exhaustion> {
        Ok(self.replacements.get(&ty).copied().unwrap_or(ty))
    }

    fn relation_demand(&self, _store: &Store, ty: TypeId) -> Option<RelationDemand> {
        (self.demand == Some(ty)).then_some(RelationDemand::Evaluation(ty))
    }
}

fn infer_type_arguments(
    interner: &mut Interner,
    next_type_param: &mut u32,
    type_params: &[TypeParamId],
    params: &[TypeId],
    args: &[TypeId],
    fresh_args: &[bool],
) -> FxHashMap<TypeParamId, TypeId> {
    let params: Vec<_> = params
        .iter()
        .enumerate()
        .map(|(index, &ty)| ParameterType::required(format!("p{index}"), ty))
        .collect();
    infer_type_arguments_from_params(
        interner,
        next_type_param,
        type_params,
        &params,
        args,
        fresh_args,
    )
}

fn infer_type_arguments_from_params(
    interner: &mut Interner,
    next_type_param: &mut u32,
    type_params: &[TypeParamId],
    params: &[ParameterType],
    args: &[TypeId],
    fresh_args: &[bool],
) -> FxHashMap<TypeParamId, TypeId> {
    let type_params: Vec<_> = type_params
        .iter()
        .map(|&id| GenericTypeParam {
            id,
            constraint: interner.store().type_param_constraint(id),
            default: None,
        })
        .collect();
    let published = PublishedClasses::empty();
    let mut queries = SemanticQueryState::default();
    let optional_undefined_fallback = vec![false; args.len()];
    match infer_signature_type_arguments_from_params(
        interner,
        next_type_param,
        &published,
        &mut queries,
        SignatureInferenceRequest {
            purpose: SignatureInferencePurpose::ClassApplication,
            type_params: &type_params,
            params,
            args,
            fresh_args,
            optional_undefined_fallback: &optional_undefined_fallback,
            receiver: None,
        },
    ) {
        DemandOutcome::Ready(result) => result.arguments,
        DemandOutcome::Exhausted(exhaustion) => {
            panic!("intrinsic-only inference unexpectedly exhausted: {exhaustion:?}")
        }
    }
}

fn measured_inference_pairs(
    interner: &mut Interner,
    count: usize,
    width: usize,
    target_param: TypeId,
) -> Vec<(TypeId, TypeId)> {
    let number = interner.well_known().number;
    (0..count)
        .map(|group| {
            let source = interner.reserve_object();
            let target = interner.reserve_object();
            let source_properties: Vec<_> = (0..width)
                .map(|index| prop(&format!("g{group:06}_p{index:02}"), number))
                .collect();
            let target_properties: Vec<_> = (0..width)
                .map(|index| prop(&format!("g{group:06}_p{index:02}"), target_param))
                .collect();
            interner.fill_object(
                source,
                ObjectType {
                    properties: source_properties,
                    ..Default::default()
                },
            );
            interner.fill_object(
                target,
                ObjectType {
                    properties: target_properties,
                    ..Default::default()
                },
            );
            (source, target)
        })
        .collect()
}

#[test]
fn measure_inference_counts_actual_snapshots_and_property_scans() {
    let mut interner = Interner::with_intrinsics();
    let target_param = interner.intern_type_param(TypeParamId(90_400), "T");
    let pairs = measured_inference_pairs(&mut interner, 2, 3, target_param);
    super::helpers::reset_inference_measure();
    for (source, target) in pairs {
        let mut candidates = Candidates::default();
        infer_from_types_for_conditional(&mut interner, source, target, &mut candidates);
    }
    assert_eq!(
        super::helpers::inference_measure(),
        super::helpers::InferenceMeasure {
            object_snapshot_vectors: 4,
            object_snapshot_entries: 12,
            object_snapshot_name_bytes: 132,
            object_target_properties: 6,
            object_source_property_comparisons: 6,
        }
    );
}

#[test]
fn ordered_object_inference_cursor_preserves_candidates_and_duplicate_parity() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t_id = TypeParamId(90_402);
    let t = interner.intern_type_param(t_id, "T");

    // The input orders are deliberately scrambled; interning establishes the stable
    // key order used by the cursor. Source-only keys occur before, between, and
    // after the target names.
    let extras_source = interner.intern_object(ObjectType {
        properties: vec![
            prop("e", wk.boolean),
            prop("b", wk.number),
            prop("c", wk.boolean),
            prop("a", wk.boolean),
            prop("d", wk.string),
        ],
        ..Default::default()
    });
    let extras_target = interner.intern_object(ObjectType {
        properties: vec![prop("d", t), prop("b", t)],
        ..Default::default()
    });

    let missing_source = interner.intern_object(ObjectType {
        properties: vec![prop("b", wk.number), prop("d", wk.string)],
        ..Default::default()
    });
    let missing_target = interner.intern_object(ObjectType {
        properties: vec![prop("d", t), prop("a", t), prop("b", t)],
        ..Default::default()
    });

    // Reserved objects admit the internal duplicate-name shape. Stable sorting and
    // first-match behavior mean both duplicate target members use the first source.
    let duplicate_source = interner.reserve_object();
    interner.fill_object(
        duplicate_source,
        ObjectType {
            properties: vec![prop("d", wk.number), prop("d", wk.string)],
            ..Default::default()
        },
    );
    let duplicate_target = interner.reserve_object();
    interner.fill_object(
        duplicate_target,
        ObjectType {
            properties: vec![prop("d", t), prop("d", t)],
            ..Default::default()
        },
    );

    super::helpers::reset_inference_measure();
    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(&mut interner, extras_source, extras_target, &mut candidates);
    assert_eq!(
        candidates.get(&t_id).map(|values| values.as_slice()),
        Some(&[wk.number, wk.string][..]),
        "target canonical order determines candidate contribution order",
    );
    assert_eq!(
        super::helpers::inference_measure().object_target_properties,
        2
    );
    assert_eq!(
        super::helpers::inference_measure().object_source_property_comparisons,
        4
    );

    super::helpers::reset_inference_measure();
    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(
        &mut interner,
        missing_source,
        missing_target,
        &mut candidates,
    );
    assert_eq!(
        candidates.get(&t_id).map(|values| values.as_slice()),
        Some(&[wk.number, wk.string][..]),
        "a missing target property contributes no candidate while later matches do",
    );
    assert_eq!(
        super::helpers::inference_measure().object_target_properties,
        3
    );
    assert_eq!(
        super::helpers::inference_measure().object_source_property_comparisons,
        3
    );

    super::helpers::reset_inference_measure();
    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(
        &mut interner,
        duplicate_source,
        duplicate_target,
        &mut candidates,
    );
    assert_eq!(
        candidates.get(&t_id).map(|values| values.as_slice()),
        Some(&[wk.number, wk.number][..]),
        "duplicate target names retain the prior first-source-member semantics",
    );
    assert_eq!(
        super::helpers::inference_measure().object_source_property_comparisons,
        1
    );

    let mut next_type_param = 90_403;
    let fixed = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[t_id],
        &[duplicate_target],
        &[duplicate_source],
        &[],
    );
    assert_eq!(
        fixed.get(&t_id).copied(),
        Some(wk.number),
        "duplicate candidates keep the existing first-source fixing result",
    );
}

#[test]
#[ignore = "WU4 release measurement; run explicitly with --ignored --nocapture"]
fn measure_inference_hotpaths_release() {
    const WIDTH: usize = 8;
    const NAME_BYTES: usize = 11;
    for count in [10_000, 100_000] {
        let mut interner = Interner::with_intrinsics();
        let target_param = interner.intern_type_param(TypeParamId(90_401), "T");
        let pairs = measured_inference_pairs(&mut interner, count, WIDTH, target_param);
        super::helpers::reset_inference_measure();
        let started = Instant::now();
        for (source, target) in pairs {
            let mut candidates = Candidates::default();
            infer_from_types_for_conditional(&mut interner, source, target, &mut candidates);
            assert_eq!(
                candidates.get(&TypeParamId(90_401)).map(Vec::len),
                Some(WIDTH)
            );
        }
        let elapsed = started.elapsed();
        let measure = super::helpers::inference_measure();
        assert_eq!(measure.object_snapshot_vectors, (count * 2) as u64);
        assert_eq!(measure.object_snapshot_entries, (count * WIDTH * 2) as u64);
        assert_eq!(
            measure.object_snapshot_name_bytes,
            (count * WIDTH * 2 * NAME_BYTES) as u64
        );
        assert_eq!(measure.object_target_properties, (count * WIDTH) as u64);
        assert_eq!(
            measure.object_source_property_comparisons,
            (count * WIDTH) as u64
        );
        println!(
            "WU4 inference count={count} width={WIDTH} elapsed_ms={} counters={measure:?}",
            elapsed.as_millis()
        );
    }
}

/// A bare scalar argument matched against a type parameter fixes that parameter
/// to the (widened) argument type: `identity(5)` infers `T = number`.
#[test]
fn infers_from_scalar_argument() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let five = interner.intern_ordinary_literal(LiteralValue::Number(5.0));
    let mut next_type_param = 1;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[t],
        &[five],
        &[],
    );
    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(wk.number),
        "T inferred from `5` widens to number"
    );
}

#[test]
fn m1_regular_candidate_dominates_same_value_widening_in_either_source_order() {
    let mut interner = Interner::with_intrinsics();
    let t_id = TypeParamId(0);
    let t = interner.intern_type_param(t_id, "T");
    let regular = interner.intern_literal(LiteralValue::Number(5.0));
    let widening = interner.intern_ordinary_literal(LiteralValue::Number(5.0));
    for args in [[regular, widening], [widening, regular]] {
        let mut next = 1;
        let map = infer_type_arguments(&mut interner, &mut next, &[t_id], &[t, t], &args, &[]);
        assert_eq!(map.get(&t_id), Some(&regular));
    }
    let mut next = 1;
    let map = infer_type_arguments(&mut interner, &mut next, &[t_id], &[t], &[regular], &[]);
    assert_eq!(map.get(&t_id), Some(&regular));
}

#[test]
fn r1a_primitive_constraint_regularizes_every_multi_candidate_terminal() {
    for (first, second) in [
        (
            LiteralValue::String("a".to_owned()),
            LiteralValue::String("b".to_owned()),
        ),
        (LiteralValue::Number(1.0), LiteralValue::Number(2.0)),
    ] {
        let mut interner = Interner::with_intrinsics();
        let parameter = TypeParamId(0);
        let target = interner.intern_type_param(parameter, "T");
        let constraint = interner.intern_intrinsic(first.base_kind());
        interner.set_type_param_constraint(parameter, constraint);
        let exact_first = interner.intern_literal(first.clone());
        let exact_second = interner.intern_literal(second.clone());
        let ordinary_first = interner.intern_ordinary_literal(first);
        let ordinary_second = interner.intern_ordinary_literal(second);
        let exact_union = interner.union(vec![exact_first, exact_second]);
        let ordinary_union = interner.union(vec![ordinary_first, ordinary_second]);
        let incompatible = interner.intern_literal(LiteralValue::Boolean(true));
        for (sources, expected) in [
            ([ordinary_first, ordinary_first], exact_first),
            ([ordinary_first, ordinary_second], exact_union),
            ([exact_first, ordinary_second], exact_union),
            ([ordinary_first, exact_second], exact_union),
            ([ordinary_first, ordinary_union], exact_union),
            ([ordinary_union, ordinary_first], exact_union),
            ([ordinary_first, incompatible], exact_first),
        ] {
            let mut next = 1;
            let result = infer_type_arguments(
                &mut interner,
                &mut next,
                &[parameter],
                &[target, target],
                &sources,
                &[],
            );
            assert_eq!(
                result.get(&parameter),
                Some(&expected),
                "sources: {sources:?}"
            );
            assert_eq!(interner.widen_literal_type(expected), expected);
        }
    }
}

#[test]
fn r1a_unconstrained_multi_candidate_modes_remain_widenable() {
    let mut interner = Interner::with_intrinsics();
    let parameter = TypeParamId(0);
    let target = interner.intern_type_param(parameter, "T");
    let first = interner.intern_ordinary_literal(LiteralValue::String("a".to_owned()));
    let second = interner.intern_ordinary_literal(LiteralValue::String("b".to_owned()));
    let expected = interner.union(vec![first, second]);
    let mut next = 1;
    let result = infer_type_arguments(
        &mut interner,
        &mut next,
        &[parameter],
        &[target, target],
        &[first, second],
        &[],
    );
    assert_eq!(result.get(&parameter), Some(&expected));
    assert_eq!(
        interner.widen_literal_type(expected),
        interner.well_known().string
    );
}

/// M25 — the **conditional collection mode**: literal candidates are NOT widened
/// (`"x"` stays `"x"`; call-site mode widens — pinned by
/// [`infers_from_scalar_argument`] above), and a **union target** descends into its
/// members (`number[]` against `string | T[]` infers `T = number`).
#[test]
fn conditional_mode_keeps_literals_and_descends_union_targets() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let x = interner.intern_literal(LiteralValue::String("x".to_string()));

    // No widening: `"x"` against `T` records the literal itself.
    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(&mut interner, x, t, &mut candidates);
    assert_eq!(
        candidates.get(&TypeParamId(0)).map(|c| c.as_slice()),
        Some(&[x][..]),
        "conditional mode must record the un-widened literal"
    );

    // Union-target descent: `number[]` against `string | T[]` lands `T = number`
    // via the array member (the string member contributes nothing).
    let t_arr = interner.intern_array(t);
    let union_target = interner.union(vec![wk.string, t_arr]);
    let num_arr = interner.intern_array(wk.number);
    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(&mut interner, num_arr, union_target, &mut candidates);
    assert_eq!(
        candidates.get(&TypeParamId(0)).map(|c| c.as_slice()),
        Some(&[wk.number][..]),
        "a union extends target must collect from its shape-matching member"
    );

    // Call-site mode also descends structural union members, without enabling
    // conditional-only template or callable-object behavior.
    struct IdentityNormalization;
    impl RelationNormalization for IdentityNormalization {
        fn normalize(&self, ty: TypeId) -> Result<TypeId, Exhaustion> {
            Ok(ty)
        }
    }
    let outcome =
        infer_from_types_for_query(&mut interner, num_arr, union_target, &IdentityNormalization);
    let InferenceAttempt::Complete(candidates) = outcome else {
        panic!("identity-normalized call-site inference must complete")
    };
    assert_eq!(
        candidates.get(&TypeParamId(0)).map(|c| c.as_slice()),
        Some(&[wk.number][..]),
        "call-site structural union members must contribute candidates"
    );
}

/// M25 round-4 — a **naked** infer union member's whole-check candidate is LOW
/// priority: it is DISCARDED when a structural member of the same union bound the
/// same binder (`{ v: T } | T` against `{ v: string }` → `T = string`, not
/// `string | { v: string }`), and KEPT when no structural member did
/// (`string | T` against `number` → `T = number`). A different-name naked member
/// never blocks a structural binder (`A | B[]`).
#[test]
fn naked_union_member_candidate_yields_to_structural() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");

    // `{ v: T } | T` against `{ v: string }` → structural wins: T = string only.
    let v_t = interner.intern_object(ObjectType {
        properties: vec![prop("v", t)],
        ..Default::default()
    });
    let target = interner.union(vec![v_t, t]);
    let v_str = interner.intern_object(ObjectType {
        properties: vec![prop("v", wk.string)],
        ..Default::default()
    });
    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(&mut interner, v_str, target, &mut candidates);
    assert_eq!(
        candidates.get(&TypeParamId(0)).map(|c| c.as_slice()),
        Some(&[wk.string][..]),
        "the naked member's whole-check candidate must be dropped"
    );

    struct IdentityNormalization;
    impl RelationNormalization for IdentityNormalization {
        fn normalize(&self, ty: TypeId) -> Result<TypeId, Exhaustion> {
            Ok(ty)
        }
    }
    let InferenceAttempt::Complete(candidates) =
        infer_from_types_for_query(&mut interner, v_str, target, &IdentityNormalization)
    else {
        panic!("identity-normalized call-site inference must complete")
    };
    assert_eq!(
        candidates.get(&TypeParamId(0)).map(|c| c.as_slice()),
        Some(&[wk.string][..]),
        "call-site structural candidates must precede naked union candidates"
    );

    // `string | T` against `number` → naked-only: the whole check IS the candidate.
    let target = interner.union(vec![wk.string, t]);
    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(&mut interner, wk.number, target, &mut candidates);
    assert_eq!(
        candidates.get(&TypeParamId(0)).map(|c| c.as_slice()),
        Some(&[wk.number][..]),
        "a naked-only member keeps its whole-check candidate"
    );

    // `A | B[]` against `number[]`: B binds structurally; the naked A (a DIFFERENT
    // binder) still records the whole check — no cross-binder blocking.
    let a = interner.intern_type_param(TypeParamId(1), "A");
    let b = interner.intern_type_param(TypeParamId(2), "B");
    let b_arr = interner.intern_array(b);
    let target = interner.union(vec![a, b_arr]);
    let num_arr = interner.intern_array(wk.number);
    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(&mut interner, num_arr, target, &mut candidates);
    assert_eq!(
        candidates.get(&TypeParamId(2)).map(|c| c.as_slice()),
        Some(&[wk.number][..]),
        "B = number from the structural member"
    );
    assert_eq!(
        candidates.get(&TypeParamId(1)).map(|c| c.as_slice()),
        Some(&[num_arr][..]),
        "A (different name) keeps its naked whole-check candidate"
    );
}

#[test]
fn query_retry_discards_partial_candidates_and_exhaustion_does_not_poison() {
    struct DemandingNormalization {
        frontier: TypeId,
    }
    impl RelationNormalization for DemandingNormalization {
        fn normalize(&self, ty: TypeId) -> Result<TypeId, Exhaustion> {
            Ok(ty)
        }

        fn relation_demand(&self, _store: &Store, ty: TypeId) -> Option<RelationDemand> {
            (ty == self.frontier).then_some(RelationDemand::Evaluation(ty))
        }
    }

    struct ExhaustingNormalization {
        frontier: TypeId,
    }
    impl RelationNormalization for ExhaustingNormalization {
        fn normalize(&self, ty: TypeId) -> Result<TypeId, Exhaustion> {
            if ty == self.frontier {
                Err(Exhaustion::EvaluationBudget)
            } else {
                Ok(ty)
            }
        }
    }

    struct IdentityNormalization;
    impl RelationNormalization for IdentityNormalization {
        fn normalize(&self, ty: TypeId) -> Result<TypeId, Exhaustion> {
            Ok(ty)
        }
    }

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let first_id = TypeParamId(90_401);
    let second_id = TypeParamId(90_402);
    let first = interner.intern_type_param(first_id, "First");
    let second = interner.intern_type_param(second_id, "Second");
    let source = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.string), prop("b", wk.number)],
        ..Default::default()
    });
    let target = interner.intern_object(ObjectType {
        properties: vec![prop("a", first), prop("b", second)],
        ..Default::default()
    });

    let mut retry = InferenceRetryState::default();
    assert!(matches!(
        retry.observe(infer_from_types_for_query(
            &mut interner,
            source,
            target,
            &DemandingNormalization {
                frontier: wk.number,
            },
        )),
        InferenceAttempt::Needs(RelationDemand::Evaluation(found)) if found == wk.number
    ));
    assert!(matches!(
        retry.observe(infer_from_types_for_query(
            &mut interner,
            source,
            target,
            &DemandingNormalization {
                frontier: wk.number,
            },
        )),
        InferenceAttempt::Exhausted(Exhaustion::EvaluationCycle { ty }) if ty == wk.number
    ));
    assert!(matches!(
        infer_from_types_for_query(
            &mut interner,
            source,
            target,
            &ExhaustingNormalization {
                frontier: wk.number,
            },
        ),
        InferenceAttempt::Exhausted(Exhaustion::EvaluationBudget)
    ));

    let InferenceAttempt::Complete(candidates) =
        infer_from_types_for_query(&mut interner, source, target, &IdentityNormalization)
    else {
        panic!("a later clean retry must complete")
    };
    assert_eq!(
        candidates.get(&first_id).map(|values| values.as_slice()),
        Some(&[wk.string][..])
    );
    assert_eq!(
        candidates.get(&second_id).map(|values| values.as_slice()),
        Some(&[wk.number][..])
    );
}

#[test]
fn distinct_declared_application_recipes_infer_by_equal_head() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let head_id = TypeParamId(90_501);
    let template = interner.intern_type_param(head_id, "H");
    let source_argument = interner.intern_declared_recipe(DeclaredRecipeNode::Type(wk.string));
    let inferred_id = TypeParamId(90_502);
    let inferred = interner.intern_type_param(inferred_id, "T");
    let target_argument = interner.intern_declared_recipe(DeclaredRecipeNode::Type(inferred));
    let source_recipe = interner.intern_declared_recipe(DeclaredRecipeNode::Application {
        template,
        parameters: vec![head_id],
        arguments: vec![source_argument],
    });
    let target_recipe = interner.intern_declared_recipe(DeclaredRecipeNode::Application {
        template,
        parameters: vec![head_id],
        arguments: vec![target_argument],
    });
    assert_ne!(source_recipe, target_recipe);
    let source = interner.intern_declared(source_recipe, []);
    let target = interner.intern_declared(target_recipe, []);
    let source_child = interner.intern_declared(source_argument, []);
    let target_child = interner.intern_declared(target_argument, []);
    let active = FxHashSet::from_iter([inferred_id]);
    let normalization = TestInferenceNormalization {
        replacements: FxHashMap::from_iter([
            (source, wk.unknown),
            (target, wk.unknown),
            (source_child, wk.string),
            (target_child, inferred),
        ]),
        demand: None,
    };

    let InferenceAttempt::Complete(candidates) = infer_from_types_for_query_with_params(
        &mut interner,
        source,
        target,
        &normalization,
        Some(&active),
    ) else {
        panic!("equal application heads must infer before root normalization")
    };
    assert_eq!(
        candidates.get(&inferred_id).map(|values| values.as_slice()),
        Some(&[wk.string][..])
    );
}

#[test]
fn nested_declared_application_heads_recurse_to_argument_candidates() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let inner_head_id = TypeParamId(90_601);
    let inner_template = interner.intern_type_param(inner_head_id, "Inner");
    let source_leaf = interner.intern_declared_recipe(DeclaredRecipeNode::Type(wk.string));
    let inferred_id = TypeParamId(90_602);
    let inferred = interner.intern_type_param(inferred_id, "T");
    let target_leaf = interner.intern_declared_recipe(DeclaredRecipeNode::Type(inferred));
    let source_inner = interner.intern_declared_recipe(DeclaredRecipeNode::Application {
        template: inner_template,
        parameters: vec![inner_head_id],
        arguments: vec![source_leaf],
    });
    let target_inner = interner.intern_declared_recipe(DeclaredRecipeNode::Application {
        template: inner_template,
        parameters: vec![inner_head_id],
        arguments: vec![target_leaf],
    });
    let outer_head_id = TypeParamId(90_603);
    let outer_template = interner.intern_type_param(outer_head_id, "Outer");
    let source_recipe = interner.intern_declared_recipe(DeclaredRecipeNode::Application {
        template: outer_template,
        parameters: vec![outer_head_id],
        arguments: vec![source_inner],
    });
    let target_recipe = interner.intern_declared_recipe(DeclaredRecipeNode::Application {
        template: outer_template,
        parameters: vec![outer_head_id],
        arguments: vec![target_inner],
    });
    let source = interner.intern_declared(source_recipe, []);
    let target = interner.intern_declared(target_recipe, []);
    let source_leaf = interner.intern_declared(source_leaf, []);
    let target_leaf = interner.intern_declared(target_leaf, []);
    let normalization = TestInferenceNormalization {
        replacements: FxHashMap::from_iter([(source_leaf, wk.string), (target_leaf, inferred)]),
        demand: None,
    };
    let active = FxHashSet::from_iter([inferred_id]);

    let InferenceAttempt::Complete(candidates) = infer_from_types_for_query_with_params(
        &mut interner,
        source,
        target,
        &normalization,
        Some(&active),
    ) else {
        panic!("nested application-head inference must complete")
    };
    assert_eq!(
        candidates.get(&inferred_id).map(|values| values.as_slice()),
        Some(&[wk.string][..])
    );
}

#[test]
fn declared_application_projection_composes_root_mappers() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let head_id = TypeParamId(90_701);
    let template = interner.intern_type_param(head_id, "H");
    let source_slot_id = TypeParamId(90_702);
    let source_slot = interner.intern_type_param(source_slot_id, "SourceSlot");
    let target_slot_id = TypeParamId(90_703);
    let target_slot = interner.intern_type_param(target_slot_id, "TargetSlot");
    let source_argument = interner.intern_declared_recipe(DeclaredRecipeNode::Type(source_slot));
    let target_argument = interner.intern_declared_recipe(DeclaredRecipeNode::Type(target_slot));
    let source_recipe = interner.intern_declared_recipe(DeclaredRecipeNode::Application {
        template,
        parameters: vec![head_id],
        arguments: vec![source_argument],
    });
    let target_recipe = interner.intern_declared_recipe(DeclaredRecipeNode::Application {
        template,
        parameters: vec![head_id],
        arguments: vec![target_argument],
    });
    let inferred_id = TypeParamId(90_704);
    let inferred = interner.intern_type_param(inferred_id, "T");
    let source = interner.intern_declared(source_recipe, [(source_slot_id, wk.string)]);
    let target = interner.intern_declared(target_recipe, [(target_slot_id, inferred)]);
    let source_child = interner.intern_declared(source_argument, [(source_slot_id, wk.string)]);
    let target_child = interner.intern_declared(target_argument, [(target_slot_id, inferred)]);
    let normalization = TestInferenceNormalization {
        replacements: FxHashMap::from_iter([(source_child, wk.string), (target_child, inferred)]),
        demand: None,
    };
    let active = FxHashSet::from_iter([inferred_id]);

    let InferenceAttempt::Complete(candidates) = infer_from_types_for_query_with_params(
        &mut interner,
        source,
        target,
        &normalization,
        Some(&active),
    ) else {
        panic!("application projection with root mappers must complete")
    };
    assert_eq!(
        candidates.get(&inferred_id).map(|values| values.as_slice()),
        Some(&[wk.string][..])
    );
}

#[test]
fn materialized_nested_application_surface_can_reveal_active_binder() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let inferred_id = TypeParamId(90_751);
    let inferred = interner.intern_type_param(inferred_id, "T");

    let nested_head_id = TypeParamId(90_752);
    let nested_parameter = interner.intern_type_param(nested_head_id, "Nested");
    let nested_template = interner.intern_object(ObjectType {
        properties: vec![prop("value", nested_parameter)],
        ..Default::default()
    });
    let nested_source_argument =
        interner.intern_declared_recipe(DeclaredRecipeNode::Type(wk.string));
    let nested_target_argument =
        interner.intern_declared_recipe(DeclaredRecipeNode::Type(inferred));
    let nested_source_recipe = interner.intern_declared_recipe(DeclaredRecipeNode::Application {
        template: nested_template,
        parameters: vec![nested_head_id],
        arguments: vec![nested_source_argument],
    });
    let nested_target_recipe = interner.intern_declared_recipe(DeclaredRecipeNode::Application {
        template: nested_template,
        parameters: vec![nested_head_id],
        arguments: vec![nested_target_argument],
    });
    let nested_source = interner.intern_declared(nested_source_recipe, []);
    let nested_target = interner.intern_declared(nested_target_recipe, []);
    let substitute::SubstitutionOutcome::CycleClean(source_surface) = interner
        .materialize_declared(nested_source)
        .expect("nested source application")
    else {
        panic!("acyclic nested source application must materialize cleanly")
    };
    let substitute::SubstitutionOutcome::CycleClean(target_surface) = interner
        .materialize_declared(nested_target)
        .expect("nested target application")
    else {
        panic!("acyclic nested target application must materialize cleanly")
    };

    let outer_head_id = TypeParamId(90_753);
    let outer_template = interner.intern_type_param(outer_head_id, "Outer");
    let outer_source_argument =
        interner.intern_declared_recipe(DeclaredRecipeNode::Type(wk.string));
    let outer_target_argument =
        interner.intern_declared_recipe(DeclaredRecipeNode::Type(wk.unknown));
    let outer_source_recipe = interner.intern_declared_recipe(DeclaredRecipeNode::Application {
        template: outer_template,
        parameters: vec![outer_head_id],
        arguments: vec![outer_source_argument],
    });
    let outer_target_recipe = interner.intern_declared_recipe(DeclaredRecipeNode::Application {
        template: outer_template,
        parameters: vec![outer_head_id],
        arguments: vec![outer_target_argument],
    });
    let source = interner.intern_declared(outer_source_recipe, []);
    let target = interner.intern_declared(outer_target_recipe, []);
    let source_child = interner.intern_declared(outer_source_argument, []);
    let target_child = interner.intern_declared(outer_target_argument, []);
    let normalization = TestInferenceNormalization {
        replacements: FxHashMap::from_iter([
            (source_child, source_surface),
            (target_child, target_surface),
        ]),
        demand: None,
    };
    let active = FxHashSet::from_iter([inferred_id]);

    let InferenceAttempt::Complete(candidates) = infer_from_types_for_query_with_params(
        &mut interner,
        source,
        target,
        &normalization,
        Some(&active),
    ) else {
        panic!("nested application surface inference must complete")
    };
    assert_eq!(
        candidates.get(&inferred_id).map(|values| values.as_slice()),
        Some(&[wk.string][..])
    );
}

#[test]
fn mismatched_declared_application_heads_fall_through_to_normalization() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let source_head_id = TypeParamId(90_801);
    let source_template = interner.intern_type_param(source_head_id, "SourceHead");
    let target_head_id = TypeParamId(90_802);
    let target_template = interner.intern_type_param(target_head_id, "TargetHead");
    let source_argument = interner.intern_declared_recipe(DeclaredRecipeNode::Type(wk.string));
    let inferred_id = TypeParamId(90_803);
    let inferred = interner.intern_type_param(inferred_id, "T");
    let target_argument = interner.intern_declared_recipe(DeclaredRecipeNode::Type(inferred));
    let source_recipe = interner.intern_declared_recipe(DeclaredRecipeNode::Application {
        template: source_template,
        parameters: vec![source_head_id],
        arguments: vec![source_argument],
    });
    let target_recipe = interner.intern_declared_recipe(DeclaredRecipeNode::Application {
        template: target_template,
        parameters: vec![target_head_id],
        arguments: vec![target_argument],
    });
    let source = interner.intern_declared(source_recipe, []);
    let target = interner.intern_declared(target_recipe, []);
    let normalization = TestInferenceNormalization {
        replacements: FxHashMap::from_iter([(source, wk.string), (target, inferred)]),
        demand: None,
    };
    let active = FxHashSet::from_iter([inferred_id]);

    let InferenceAttempt::Complete(candidates) = infer_from_types_for_query_with_params(
        &mut interner,
        source,
        target,
        &normalization,
        Some(&active),
    ) else {
        panic!("mismatched application heads must use normalized fallback")
    };
    assert_eq!(
        candidates.get(&inferred_id).map(|values| values.as_slice()),
        Some(&[wk.string][..])
    );
}

#[test]
fn cyclic_declared_application_child_normalization_terminates() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let head_id = TypeParamId(90_901);
    let template = interner.intern_type_param(head_id, "H");
    let source_argument = interner.intern_declared_recipe(DeclaredRecipeNode::Type(wk.string));
    let inferred_id = TypeParamId(90_902);
    let inferred = interner.intern_type_param(inferred_id, "T");
    let target_argument = interner.intern_declared_recipe(DeclaredRecipeNode::Type(inferred));
    let source_recipe = interner.intern_declared_recipe(DeclaredRecipeNode::Application {
        template,
        parameters: vec![head_id],
        arguments: vec![source_argument],
    });
    let target_recipe = interner.intern_declared_recipe(DeclaredRecipeNode::Application {
        template,
        parameters: vec![head_id],
        arguments: vec![target_argument],
    });
    let source = interner.intern_declared(source_recipe, []);
    let target = interner.intern_declared(target_recipe, []);
    let source_child = interner.intern_declared(source_argument, []);
    let target_child = interner.intern_declared(target_argument, []);
    let normalization = TestInferenceNormalization {
        replacements: FxHashMap::from_iter([(source_child, source), (target_child, target)]),
        demand: None,
    };
    let active = FxHashSet::from_iter([inferred_id]);

    let InferenceAttempt::Complete(candidates) = infer_from_types_for_query_with_params(
        &mut interner,
        source,
        target,
        &normalization,
        Some(&active),
    ) else {
        panic!("raw application recursion guard must terminate normalization cycles")
    };
    assert!(candidates.is_empty());
}

#[test]
fn inactive_declared_application_children_do_not_trigger_demands() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let first_head_id = TypeParamId(91_001);
    let second_head_id = TypeParamId(91_002);
    let first_head = interner.intern_type_param(first_head_id, "A");
    let second_head = interner.intern_type_param(second_head_id, "B");
    let template = interner.intern_tuple(vec![first_head, second_head]);
    let source_foreign = interner.intern_declared_recipe(DeclaredRecipeNode::Type(wk.number));
    let source_active = interner.intern_declared_recipe(DeclaredRecipeNode::Type(wk.string));
    let foreign_id = TypeParamId(91_003);
    let foreign = interner.intern_type_param(foreign_id, "Foreign");
    let target_foreign = interner.intern_declared_recipe(DeclaredRecipeNode::Type(foreign));
    let inferred_id = TypeParamId(91_004);
    let inferred = interner.intern_type_param(inferred_id, "T");
    let target_active = interner.intern_declared_recipe(DeclaredRecipeNode::Type(inferred));
    let source_recipe = interner.intern_declared_recipe(DeclaredRecipeNode::Application {
        template,
        parameters: vec![first_head_id, second_head_id],
        arguments: vec![source_foreign, source_active],
    });
    let target_recipe = interner.intern_declared_recipe(DeclaredRecipeNode::Application {
        template,
        parameters: vec![first_head_id, second_head_id],
        arguments: vec![target_foreign, target_active],
    });
    let source = interner.intern_declared(source_recipe, []);
    let target = interner.intern_declared(target_recipe, []);
    let source_foreign = interner.intern_declared(source_foreign, []);
    let target_foreign = interner.intern_declared(target_foreign, []);
    let source_active = interner.intern_declared(source_active, []);
    let target_active = interner.intern_declared(target_active, []);
    let normalization = TestInferenceNormalization {
        replacements: FxHashMap::from_iter([
            (source_foreign, wk.number),
            (target_foreign, foreign),
            (source_active, wk.string),
            (target_active, inferred),
        ]),
        demand: Some(wk.number),
    };
    let active = FxHashSet::from_iter([inferred_id]);

    let InferenceAttempt::Complete(candidates) = infer_from_types_for_query_with_params(
        &mut interner,
        source,
        target,
        &normalization,
        Some(&active),
    ) else {
        panic!("inactive argument recipes must be skipped before normalization demand")
    };
    assert!(!candidates.contains_key(&foreign_id));
    assert_eq!(
        candidates.get(&inferred_id).map(|values| values.as_slice()),
        Some(&[wk.string][..])
    );
}

#[test]
fn distinct_substitution_generations_terminate_and_keep_earlier_candidate() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let source_link_id = TypeParamId(91_101);
    let target_link_id = TypeParamId(91_102);
    let inferred_id = TypeParamId(91_103);
    let source_link = interner.intern_type_param(source_link_id, "SourceLink");
    let target_link = interner.intern_type_param(target_link_id, "TargetLink");
    let inferred = interner.intern_type_param(inferred_id, "T");
    let source_template = interner.intern_object(ObjectType {
        properties: vec![prop("a_value", wk.string), prop("z_next", source_link)],
        ..Default::default()
    });
    let target_template = interner.intern_object(ObjectType {
        properties: vec![prop("a_value", inferred), prop("z_next", target_link)],
        ..Default::default()
    });
    let source_leaf = interner.intern_object(ObjectType::default());
    let target_leaf = interner.intern_object(ObjectType {
        properties: vec![prop("end", wk.number)],
        ..Default::default()
    });
    let source_child = substitute(
        &mut interner,
        source_template,
        &FxHashMap::from_iter([(source_link_id, source_leaf)]),
    );
    let target_child = substitute(
        &mut interner,
        target_template,
        &FxHashMap::from_iter([(target_link_id, target_leaf)]),
    );
    let source = substitute(
        &mut interner,
        source_template,
        &FxHashMap::from_iter([(source_link_id, source_child)]),
    );
    let target = substitute(
        &mut interner,
        target_template,
        &FxHashMap::from_iter([(target_link_id, target_child)]),
    );
    assert_ne!(source, source_child);
    assert_ne!(target, target_child);
    let active = FxHashSet::from_iter([inferred_id]);

    let InferenceAttempt::Complete(candidates) = infer_from_types_for_query_with_params(
        &mut interner,
        source,
        target,
        &TestInferenceNormalization::default(),
        Some(&active),
    ) else {
        panic!("stable recursion identity must terminate without exhaustion")
    };
    let inferred = candidates
        .get(&inferred_id)
        .expect("the sibling visited before the circular branch must survive");
    assert!(!inferred.is_empty());
    assert!(inferred.iter().all(|candidate| *candidate == wk.string));
}

#[test]
fn two_nested_derived_applications_reach_the_inner_candidate() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let source_link_id = TypeParamId(91_105);
    let target_link_id = TypeParamId(91_106);
    let inferred_id = TypeParamId(91_107);
    let source_link = interner.intern_type_param(source_link_id, "SourceLink");
    let target_link = interner.intern_type_param(target_link_id, "TargetLink");
    let inferred = interner.intern_type_param(inferred_id, "T");
    let source_template = interner.intern_object(ObjectType {
        properties: vec![prop("value", source_link)],
        ..Default::default()
    });
    let target_template = interner.intern_object(ObjectType {
        properties: vec![prop("value", target_link)],
        ..Default::default()
    });
    let source_child = crate::types::substitute_derived(
        &mut interner,
        source_template,
        &FxHashMap::from_iter([(source_link_id, DerivedType::plain(wk.string))]),
    );
    let target_child = crate::types::substitute_derived(
        &mut interner,
        target_template,
        &FxHashMap::from_iter([(target_link_id, DerivedType::plain(inferred))]),
    );
    let source = crate::types::substitute_derived(
        &mut interner,
        source_template,
        &FxHashMap::from_iter([(source_link_id, source_child)]),
    );
    let target = crate::types::substitute_derived(
        &mut interner,
        target_template,
        &FxHashMap::from_iter([(target_link_id, target_child)]),
    );

    let InferenceAttempt::Complete(candidates) = infer_from_derived_types_for_query(
        &mut interner,
        source,
        target,
        &TestInferenceNormalization::default(),
    ) else {
        panic!("two nested applications must complete")
    };
    assert_eq!(
        candidates.get(&inferred_id).map(|values| values.as_slice()),
        Some(&[wk.string][..])
    );
}

#[test]
fn one_sided_template_repeat_does_not_cut_structural_inference() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let source_link_id = TypeParamId(91_111);
    let root_link_id = TypeParamId(91_112);
    let child_link_id = TypeParamId(91_113);
    let inferred_id = TypeParamId(91_114);
    let source_link = interner.intern_type_param(source_link_id, "SourceLink");
    let root_link = interner.intern_type_param(root_link_id, "RootLink");
    let child_link = interner.intern_type_param(child_link_id, "ChildLink");
    let inferred = interner.intern_type_param(inferred_id, "T");
    let source_template = interner.intern_object(ObjectType {
        properties: vec![prop("a_value", wk.string), prop("z_next", source_link)],
        ..Default::default()
    });
    let target_root_template = interner.intern_object(ObjectType {
        properties: vec![prop("m_root", wk.number), prop("z_next", root_link)],
        ..Default::default()
    });
    let target_child_template = interner.intern_object(ObjectType {
        properties: vec![
            prop("a_value", inferred),
            prop("m_child", wk.string),
            prop("z_next", child_link),
        ],
        ..Default::default()
    });
    let source_leaf = interner.intern_object(ObjectType::default());
    let target_leaf = interner.intern_object(ObjectType {
        properties: vec![prop("end", wk.boolean)],
        ..Default::default()
    });
    let source_child = substitute(
        &mut interner,
        source_template,
        &FxHashMap::from_iter([(source_link_id, source_leaf)]),
    );
    let source = substitute(
        &mut interner,
        source_template,
        &FxHashMap::from_iter([(source_link_id, source_child)]),
    );
    let target_child = substitute(
        &mut interner,
        target_child_template,
        &FxHashMap::from_iter([(child_link_id, target_leaf)]),
    );
    let target = substitute(
        &mut interner,
        target_root_template,
        &FxHashMap::from_iter([(root_link_id, target_child)]),
    );
    let active = FxHashSet::from_iter([inferred_id]);

    let InferenceAttempt::Complete(candidates) = infer_from_types_for_query_with_params(
        &mut interner,
        source,
        target,
        &TestInferenceNormalization::default(),
        Some(&active),
    ) else {
        panic!("one-sided expansion must continue")
    };
    assert_eq!(
        candidates.get(&inferred_id).map(|values| values.as_slice()),
        Some(&[wk.string][..])
    );
}

#[test]
fn unrelated_anonymous_structures_do_not_share_recursion_identity() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let inferred_id = TypeParamId(91_121);
    let inferred = interner.intern_type_param(inferred_id, "T");
    let source_inner = interner.intern_object(ObjectType {
        properties: vec![prop("a_value", wk.string)],
        ..Default::default()
    });
    let target_inner = interner.intern_object(ObjectType {
        properties: vec![prop("a_value", inferred)],
        ..Default::default()
    });
    let source = interner.intern_object(ObjectType {
        properties: vec![prop("z_next", source_inner)],
        ..Default::default()
    });
    let target = interner.intern_object(ObjectType {
        properties: vec![prop("z_next", target_inner)],
        ..Default::default()
    });
    let active = FxHashSet::from_iter([inferred_id]);

    let InferenceAttempt::Complete(candidates) = infer_from_types_for_query_with_params(
        &mut interner,
        source,
        target,
        &TestInferenceNormalization::default(),
        Some(&active),
    ) else {
        panic!("anonymous structural inference must complete")
    };
    assert_eq!(
        candidates.get(&inferred_id).map(|values| values.as_slice()),
        Some(&[wk.string][..])
    );
}

/// A candidate matched against a non-generic parameter is not recorded, but the
/// return-bearing parameter still infers: `pick(1, \"x\")` infers `A = number`,
/// `B = string` (each from its own parameter).
#[test]
fn infers_each_parameter_independently() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let a = interner.intern_type_param(TypeParamId(0), "A");
    let b = interner.intern_type_param(TypeParamId(1), "B");
    let one = interner.intern_ordinary_literal(LiteralValue::Number(1.0));
    let x = interner.intern_ordinary_literal(LiteralValue::String("x".to_string()));
    let mut next_type_param = 2;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0), TypeParamId(1)],
        &[a, b],
        &[one, x],
        &[],
    );
    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(wk.number),
        "A = number"
    );
    assert_eq!(
        map.get(&TypeParamId(1)).copied(),
        Some(wk.string),
        "B = string"
    );
}

/// A type parameter nested inside an object parameter is inferred from the
/// matching property of the argument object: `unwrap({ value: 1 })` with the
/// parameter `{ value: T }` infers `T = number`. (Object-literal members arrive
/// already widened, so the candidate is `number` here.)
#[test]
fn infers_from_object_property() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    // Parameter `{ value: T }`.
    let box_t = interner.intern_object(ObjectType {
        properties: vec![prop("value", t)],
        ..Default::default()
    });
    // Argument `{ value: number }` (member already widened by the checker).
    let arg = interner.intern_object(ObjectType {
        properties: vec![prop("value", wk.number)],
        ..Default::default()
    });
    let mut next_type_param = 1;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[box_t],
        &[arg],
        &[],
    );
    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(wk.number),
        "T = number"
    );
}

#[test]
fn object_inference_matches_iterator_and_async_iterator_by_exact_key() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let parameter_id = TypeParamId(90_301);
    let parameter = interner.intern_type_param(parameter_id, "T");
    let iterator_target = interner.intern_object(ObjectType {
        properties: vec![PropertyType::well_known_symbol(
            WellKnownSymbol::Iterator,
            parameter,
        )],
        ..Default::default()
    });
    let iterator_source = interner.intern_object(ObjectType {
        properties: vec![PropertyType::well_known_symbol(
            WellKnownSymbol::Iterator,
            wk.string,
        )],
        ..Default::default()
    });
    let async_source = interner.intern_object(ObjectType {
        properties: vec![PropertyType::well_known_symbol(
            WellKnownSymbol::AsyncIterator,
            wk.number,
        )],
        ..Default::default()
    });
    let mut next_type_param = 90_302;

    let matched = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[parameter_id],
        &[iterator_target],
        &[iterator_source],
        &[],
    );
    assert_eq!(matched.get(&parameter_id), Some(&wk.string));

    let mismatched = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[parameter_id],
        &[iterator_target],
        &[async_source],
        &[],
    );
    assert_eq!(mismatched.get(&parameter_id), Some(&wk.unknown));
}

/// A type parameter under a function parameter is inferred from both the
/// parameter positions and the return type.
#[test]
fn infers_through_function_parameter() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    // Parameter type `(x: T) => T`.
    let target = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required("x", t)],
        ret: t,
    });
    // Argument type `(x: number) => number`.
    let source = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required("x", wk.number)],
        ret: wk.number,
    });
    let mut next_type_param = 1;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[target],
        &[source],
        &[],
    );
    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(wk.number),
        "T = number"
    );
}

#[test]
fn call_site_recovery_return_does_not_override_a_signature_default() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let result_id = TypeParamId(90_320);
    let result = interner.intern_type_param(result_id, "Result");
    let source = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: Vec::new(),
        ret: wk.error,
    });
    let target = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: Vec::new(),
        ret: result,
    });
    let active = FxHashSet::from_iter([result_id]);

    let InferenceAttempt::Complete(candidates) = infer_from_types_for_query_with_params(
        &mut interner,
        source,
        target,
        &TestInferenceNormalization::default(),
        Some(&active),
    ) else {
        panic!("intrinsic recovery inference must complete")
    };

    assert!(
        !candidates.contains_key(&result_id),
        "the recovery type is not evidence for replacing a declared default"
    );
}

/// A function receiver is a structural child for inference but never a positional
/// call argument.
#[test]
fn infers_through_function_receiver() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let target = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: Some(t),
        params: vec![ParameterType::required("value", wk.unknown)],
        ret: wk.unknown,
    });
    let source = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: Some(wk.number),
        params: vec![ParameterType::required("value", wk.unknown)],
        ret: wk.unknown,
    });
    let mut next_type_param = 1;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[target],
        &[source],
        &[],
    );
    assert_eq!(map.get(&TypeParamId(0)).copied(), Some(wk.number));
}

#[test]
fn conditional_infer_callable_object_uses_last_call_signature() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let r = interner.intern_type_param(TypeParamId(0), "R");
    let target = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: Vec::new(),
        ret: r,
    });
    let first = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required("value", wk.string)],
        ret: wk.number,
    });
    let last = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required("value", wk.number)],
        ret: wk.string,
    });
    let source = interner.intern_object(ObjectType {
        properties: vec![prop("tag", wk.boolean)],
        call_signatures: vec![first, last],
        ..Default::default()
    });

    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(&mut interner, source, target, &mut candidates);

    assert_eq!(
        candidates.get(&TypeParamId(0)).map(|c| c.as_slice()),
        Some(&[wk.string][..]),
        "only the final call signature contributes the inferred return",
    );
}

/// Incompatible candidates from different arguments do not union into a target
/// wide enough to accept both; fixing keeps a replay target that will reject the
/// later string argument.
#[test]
fn incompatible_multi_source_candidates_fix_to_first_prepared() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let one = interner.intern_ordinary_literal(LiteralValue::Number(1.0));
    let s = interner.intern_ordinary_literal(LiteralValue::String("s".to_string()));
    let mut next_type_param = 1;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[t, t],
        &[one, s],
        &[],
    );
    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(wk.number),
        "T fixes to number so the replayed string argument can fail"
    );
}

/// Compatible same-family literal candidates from different arguments keep their
/// literal union, matching `both(1, 2)` returning `1 | 2`.
#[test]
fn same_family_literal_candidates_union() {
    let mut interner = Interner::with_intrinsics();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let one = interner.intern_literal(LiteralValue::Number(1.0));
    let two = interner.intern_literal(LiteralValue::Number(2.0));
    let expected = interner.union(vec![one, two]);
    let mut next_type_param = 1;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[t, t],
        &[one, two],
        &[],
    );
    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(expected),
        "T = 1 | 2"
    );
}

#[test]
fn enum_candidates_merge_only_with_their_own_declaration_family() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let (_, first_numeric) = intern_enum_family(
        &mut interner,
        EnumId(10),
        "FirstNumeric",
        vec![
            EnumValue::Number(EnumNumber::new(1.0).expect("finite enum constant")),
            EnumValue::Number(EnumNumber::new(2.0).expect("finite enum constant")),
        ],
        false,
    );
    let (_, second_numeric) = intern_enum_family(
        &mut interner,
        EnumId(11),
        "SecondNumeric",
        vec![EnumValue::Number(
            EnumNumber::new(3.0).expect("finite enum constant"),
        )],
        false,
    );
    let (_, first_string) = intern_enum_family(
        &mut interner,
        EnumId(12),
        "FirstString",
        vec![EnumValue::String("enum".to_owned())],
        false,
    );
    let raw_number = interner.intern_literal(LiteralValue::Number(99.0));
    let raw_string = interner.intern_literal(LiteralValue::String("raw".to_owned()));
    let same_enum = interner.union(vec![first_numeric[0], first_numeric[1]]);

    assert_eq!(
        same_primitive_family(interner.store(), first_numeric[0], first_numeric[1]),
        Some(PrimitiveFamily::Number),
        "members of one enum may form a same-family inference candidate",
    );
    assert_eq!(
        same_primitive_family(interner.store(), same_enum, first_numeric[0]),
        Some(PrimitiveFamily::Number),
        "same-enum provenance survives recursive unions",
    );
    assert_eq!(
        same_primitive_family(interner.store(), first_numeric[0], raw_number),
        None,
        "a numeric enum and a raw numeric literal stay nominally distinct",
    );
    assert_eq!(
        same_primitive_family(interner.store(), first_numeric[0], second_numeric[0]),
        None,
        "different numeric enum declarations stay nominally distinct",
    );
    assert_eq!(
        same_primitive_family(interner.store(), first_string[0], raw_string),
        None,
        "a string enum and a raw string literal stay nominally distinct",
    );

    let t_id = TypeParamId(0);
    let t = interner.intern_type_param(t_id, "T");
    let mut next_type_param = 1;
    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[t_id],
        &[t, t],
        &[first_numeric[0], first_numeric[1]],
        &[],
    );
    assert_eq!(map.get(&t_id).copied(), Some(same_enum));

    for incompatible in [raw_number, second_numeric[0]] {
        let mut next_type_param = 1;
        let map = infer_type_arguments(
            &mut interner,
            &mut next_type_param,
            &[t_id],
            &[t, t],
            &[first_numeric[0], incompatible],
            &[],
        );
        assert_eq!(
            map.get(&t_id).copied(),
            Some(first_numeric[0]),
            "the first enum candidate remains the replay target",
        );
    }

    let s_id = TypeParamId(1);
    let s = interner.intern_type_param(s_id, "S");
    let mut next_type_param = 2;
    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[s_id],
        &[s, s],
        &[first_string[0], raw_string],
        &[],
    );
    assert_eq!(map.get(&s_id).copied(), Some(first_string[0]));

    let c_id = TypeParamId(2);
    let c = interner.intern_type_param(c_id, "C");
    interner.set_type_param_constraint(c_id, wk.number);
    let mut next_type_param = 3;
    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[c_id],
        &[c, c],
        &[first_numeric[0], raw_number],
        &[],
    );
    assert_eq!(
        map.get(&c_id).copied(),
        Some(first_numeric[0]),
        "a primitive constraint must not erase enum provenance",
    );
}

#[test]
fn enum_primitive_controls_keep_nullish_heterogeneous_and_open_behavior() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let (heterogeneous, _) = intern_enum_family(
        &mut interner,
        EnumId(20),
        "Heterogeneous",
        vec![
            EnumValue::Number(EnumNumber::new(1.0).expect("finite enum constant")),
            EnumValue::String("ready".to_owned()),
        ],
        false,
    );
    let (empty_open, _) =
        intern_enum_family(&mut interner, EnumId(21), "EmptyOpen", Vec::new(), true);

    assert_eq!(
        same_primitive_family(interner.store(), heterogeneous, heterogeneous),
        None,
        "a heterogeneous enum does not collapse to one primitive family",
    );
    assert_eq!(
        same_primitive_family(interner.store(), empty_open, empty_open),
        Some(PrimitiveFamily::Number),
        "an empty open enum retains numeric facts and declaration identity",
    );
    assert_eq!(
        same_primitive_family(interner.store(), empty_open, wk.number),
        None,
        "numeric openness does not make the enum a raw number candidate",
    );

    let t_id = TypeParamId(0);
    let t = interner.intern_type_param(t_id, "T");
    let expected = interner.union(vec![empty_open, wk.null]);
    let mut next_type_param = 1;
    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[t_id],
        &[t, t],
        &[empty_open, wk.null],
        &[],
    );
    assert_eq!(
        map.get(&t_id).copied(),
        Some(expected),
        "nullish merging remains independent of primitive-family merging",
    );
}

#[test]
fn nullish_candidates_do_not_launder_enum_provenance() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let (_, first) = intern_enum_family(
        &mut interner,
        EnumId(30),
        "First",
        vec![EnumValue::Number(
            EnumNumber::new(1.0).expect("finite enum constant"),
        )],
        false,
    );
    let (_, second) = intern_enum_family(
        &mut interner,
        EnumId(31),
        "Second",
        vec![EnumValue::Number(
            EnumNumber::new(1.0).expect("finite enum constant"),
        )],
        false,
    );
    let raw = interner.intern_ordinary_literal(LiteralValue::Number(99.0));
    let matching_raw = interner.intern_literal(LiteralValue::Number(1.0));
    let first_null = interner.union(vec![first[0], wk.null]);
    let second_null = interner.union(vec![second[0], wk.null]);
    let raw_null = interner.union(vec![raw, wk.null]);
    let t_id = TypeParamId(0);
    let t = interner.intern_type_param(t_id, "T");

    let cases = [
        ([first[0], wk.null, second[0]], first_null),
        ([wk.null, first[0], second[0]], first_null),
        ([first[0], second[0], wk.null], first[0]),
        ([second[0], wk.null, first[0]], second_null),
        ([wk.null, second[0], first[0]], second_null),
        ([second[0], first[0], wk.null], second[0]),
        ([first[0], wk.null, raw], first_null),
        ([wk.null, first[0], raw], first_null),
        ([first[0], raw, wk.null], first[0]),
        ([raw, wk.null, first[0]], raw_null),
        ([wk.null, raw, first[0]], raw_null),
        ([raw, first[0], wk.null], wk.number),
    ];
    for (args, expected) in cases {
        let mut next_type_param = 1;
        let map = infer_type_arguments(
            &mut interner,
            &mut next_type_param,
            &[t_id],
            &[t, t, t],
            &args,
            &[],
        );
        assert_eq!(
            map.get(&t_id).copied(),
            Some(expected),
            "nullish evidence must not bridge raw or cross-enum candidates: {args:?}",
        );
    }

    for args in [
        [first[0], wk.null, first[0]],
        [first[0], wk.undefined, first[0]],
        [first[0], wk.null, wk.undefined],
        [first[0], wk.null, matching_raw],
    ] {
        let mut next_type_param = 1;
        let map = infer_type_arguments(
            &mut interner,
            &mut next_type_param,
            &[t_id],
            &[t, t, t],
            &args,
            &[],
        );
        assert_eq!(
            enum_provenance(interner.store(), map[&t_id]),
            EnumProvenance::One(EnumId(30)),
            "compatible enum and nullish evidence must retain enum identity: {args:?}",
        );
    }
}

/// A type parameter with **no** candidate falls back to `unknown` (the sound
/// fallback), never `any`. Here the argument shape (a scalar) does not match the
/// parameter shape (an object), so nothing is inferred for `T`.
#[test]
fn no_candidate_falls_back_to_unknown() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    // Parameter `{ value: T }`; argument is a bare `number` (shape mismatch).
    let box_t = interner.intern_object(ObjectType {
        properties: vec![prop("value", t)],
        ..Default::default()
    });
    let mut next_type_param = 1;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[box_t],
        &[wk.number],
        &[],
    );
    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(wk.unknown),
        "no candidate → unknown, never any"
    );
    assert_ne!(map.get(&TypeParamId(0)).copied(), Some(wk.any));
}

#[test]
fn call_site_candidates_remember_argument_sources() {
    let mut interner = Interner::with_intrinsics();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let one = interner.intern_literal(LiteralValue::Number(1.0));
    let s = interner.intern_literal(LiteralValue::String("s".to_string()));

    let candidates = collect_call_site_candidates(
        &mut interner,
        &[
            ParameterType::required("a", t),
            ParameterType::required("b", t),
        ],
        &[one, s],
        &[true, false],
        None,
    );

    let cands = candidates
        .get(&TypeParamId(0))
        .expect("T receives candidates");
    assert_eq!(cands.len(), 2);
    assert_eq!(cands[0].ty, one);
    assert_eq!(
        cands[0].source,
        CallSiteSource::Argument {
            index: 0,
            occurrence: 0
        }
    );
    assert!(cands[0].fresh);
    assert_eq!(cands[1].ty, s);
    assert_eq!(
        cands[1].source,
        CallSiteSource::Argument {
            index: 1,
            occurrence: 0
        }
    );
    assert!(!cands[1].fresh);
}

#[test]
fn call_site_tuple_expansion_keeps_distinct_occurrences() {
    let mut interner = Interner::with_intrinsics();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let t_array = interner.intern_array(t);
    let one = interner.intern_literal(LiteralValue::Number(1.0));
    let two = interner.intern_literal(LiteralValue::Number(2.0));
    let tuple = interner.intern_tuple(vec![one, two]);

    let candidates = collect_call_site_candidates(
        &mut interner,
        &[ParameterType::required("items", t_array)],
        &[tuple],
        &[false],
        None,
    );

    let cands = candidates
        .get(&TypeParamId(0))
        .expect("T receives tuple element candidates");
    assert_eq!(cands.len(), 2);
    assert_eq!(cands[0].ty, one);
    assert_eq!(
        cands[0].source,
        CallSiteSource::Argument {
            index: 0,
            occurrence: 0
        }
    );
    assert_eq!(cands[1].ty, two);
    assert_eq!(
        cands[1].source,
        CallSiteSource::Argument {
            index: 0,
            occurrence: 1
        }
    );
}

#[test]
fn single_argument_occurrences_do_not_union_incompatible_candidates() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let target = interner.intern_object(ObjectType {
        properties: vec![prop("a", t), prop("b", t)],
        ..Default::default()
    });
    let source = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number), prop("b", wk.string)],
        ..Default::default()
    });
    let mut next_type_param = 1;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[target],
        &[source],
        &[],
    );

    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(wk.number),
        "the second property replays against number and fails"
    );
}

#[test]
fn nonprimitive_multi_source_candidates_do_not_union() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let object_x = interner.intern_object(ObjectType {
        properties: vec![prop("x", wk.number)],
        ..Default::default()
    });
    let object_y = interner.intern_object(ObjectType {
        properties: vec![prop("y", wk.number)],
        ..Default::default()
    });
    let mut next_type_param = 1;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[t, t],
        &[object_x, object_y],
        &[],
    );

    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(object_x),
        "typed object candidates from separate arguments must replay"
    );
}

#[test]
fn fresh_structural_candidate_yields_to_typed_candidate() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let fresh_object = interner.intern_object(ObjectType {
        properties: vec![prop("x", wk.number)],
        ..Default::default()
    });
    let typed_object = interner.intern_object(ObjectType {
        properties: vec![prop("x", wk.number), prop("y", wk.number)],
        ..Default::default()
    });
    let mut next_type_param = 1;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[t, t],
        &[fresh_object, typed_object],
        &[true, false],
    );

    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(typed_object),
        "the typed structural candidate replays the earlier fresh literal"
    );
}

#[test]
fn nullish_multi_source_candidates_merge() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let expected = interner.union(vec![wk.null, wk.undefined]);
    let mut next_type_param = 1;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[t, t, t],
        &[wk.null, wk.null, wk.undefined],
        &[],
    );

    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(expected),
        "null and undefined same-T candidates merge"
    );
}

#[test]
fn nullish_and_single_primitive_family_candidates_merge() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let s = interner.intern_literal(LiteralValue::String("s".to_string()));
    let expected = interner.union(vec![wk.null, s]);
    let mut next_type_param = 1;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[t, t],
        &[wk.null, s],
        &[],
    );

    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(expected),
        "nullish plus one primitive family merges, unlike number plus string"
    );

    let u = interner.intern_type_param(TypeParamId(1), "U");
    let constrained = interner.union(vec![wk.null, wk.undefined, wk.string]);
    interner.set_type_param_constraint(TypeParamId(1), constrained);
    let expected = interner.union(vec![s, wk.undefined]);
    let mut next_type_param = 2;

    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(1)],
        &[u, u],
        &[s, wk.undefined],
        &[],
    );

    assert_eq!(
        map.get(&TypeParamId(1)).copied(),
        Some(expected),
        "primitive-constrained nullish candidates still merge with string"
    );
}

#[test]
fn call_site_rest_array_candidates_keep_argument_sources() {
    let mut interner = Interner::with_intrinsics();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let t_array = interner.intern_array(t);
    let one = interner.intern_literal(LiteralValue::Number(1.0));
    let two = interner.intern_literal(LiteralValue::Number(2.0));

    let candidates = collect_call_site_candidates(
        &mut interner,
        &[ParameterType::rest("args", t_array)],
        &[one, two],
        &[false, false],
        None,
    );

    let cands = candidates
        .get(&TypeParamId(0))
        .expect("T receives rest element candidates");
    assert_eq!(cands.len(), 2);
    assert_eq!(
        cands[0].source,
        CallSiteSource::Argument {
            index: 0,
            occurrence: 0
        }
    );
    assert_eq!(
        cands[1].source,
        CallSiteSource::Argument {
            index: 1,
            occurrence: 0
        }
    );
}

/// M24/M27 — a parameter with a **primitive constraint** keeps the inferred literal
/// (tsc `hasPrimitiveConstraint`): `mk<T extends string>("x")` infers `T = "x"`, while
/// an unconstrained `id<U>("x")` widens to `string`.
#[test]
fn primitive_constraint_preserves_literal() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let x = interner.intern_literal(LiteralValue::String("x".to_string()));

    // `<T extends string>`: literal preserved.
    let t = interner.intern_type_param(TypeParamId(0), "T");
    interner.set_type_param_constraint(TypeParamId(0), wk.string);
    let mut next_type_param = 1;
    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[t],
        &[x],
        &[],
    );
    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(x),
        "a string-constrained parameter keeps the literal `\"x\"`"
    );

    // `<U>` (unconstrained): widened.
    let x = interner.intern_ordinary_literal(LiteralValue::String("x".to_string()));
    let u = interner.intern_type_param(TypeParamId(1), "U");
    let mut next_type_param = 2;
    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(1)],
        &[u],
        &[x],
        &[],
    );
    assert_eq!(
        map.get(&TypeParamId(1)).copied(),
        Some(wk.string),
        "an unconstrained parameter widens `\"x\"` → string"
    );
}

/// M27 — template-pattern `infer` capture: matching a string literal against a
/// template extends target captures each `infer` hole (a freshened type parameter) as
/// a NON-widened string-literal candidate, non-greedily on the first separator; a
/// failed anchor records nothing.
#[test]
fn template_infer_captures_segments() {
    use crate::types::repr::TemplateType;
    let mut interner = Interner::with_intrinsics();

    // Extends pattern `` `${L}:${R}` `` with L, R freshened infer parameters.
    let l = interner.intern_type_param(TypeParamId(0), "L");
    let r = interner.intern_type_param(TypeParamId(1), "R");
    let pattern = interner.intern_template(TemplateType {
        texts: vec![String::new(), ":".to_string(), String::new()],
        holes: vec![l, r],
    });

    // "a:b:c" — first `:` anchors (non-greedy): L = "a", R = "b:c".
    let check = interner.intern_literal(LiteralValue::String("a:b:c".to_string()));
    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(&mut interner, check, pattern, &mut candidates);
    let a = interner.intern_literal(LiteralValue::String("a".to_string()));
    let bc = interner.intern_literal(LiteralValue::String("b:c".to_string()));
    assert_eq!(
        candidates.get(&TypeParamId(0)).map(|c| c.as_slice()),
        Some(&[a][..]),
        "L = \"a\""
    );
    assert_eq!(
        candidates.get(&TypeParamId(1)).map(|c| c.as_slice()),
        Some(&[bc][..]),
        "R = \"b:c\""
    );

    // A source with no `:` separator records nothing (no match → false branch).
    let no_sep = interner.intern_literal(LiteralValue::String("abc".to_string()));
    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(&mut interner, no_sep, pattern, &mut candidates);
    assert!(
        candidates.is_empty(),
        "a non-matching source records no candidate"
    );
}

#[test]
fn constrained_template_infer_uses_typescript_numeric_capture_rules() {
    use crate::types::repr::TemplateType;
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let parameter_id = TypeParamId(0);
    let parameter = interner.intern_type_param(parameter_id, "N");
    let pattern = interner.intern_template(TemplateType {
        texts: vec![String::new(), String::new()],
        holes: vec![parameter],
    });
    let constraints = FxHashMap::from_iter([(parameter_id, wk.number)]);

    for (source, expected) in [
        ("-1", Some(LiteralValue::Number(-1.0))),
        ("01", None),
        ("\u{00A0}", None),
        ("\u{FEFF}", None),
        ("\u{00A0}1\u{00A0}", None),
        ("\u{FEFF}1\u{FEFF}", None),
    ] {
        let source = interner.intern_literal(LiteralValue::String(source.to_owned()));
        let mut candidates = Candidates::default();
        infer_from_types_for_constrained_conditional(
            &mut interner,
            source,
            pattern,
            &constraints,
            &mut candidates,
        );
        let candidate = candidates[&parameter_id][0];
        match expected {
            Some(expected) => {
                assert_eq!(interner.store().literal_value(candidate), Some(&expected))
            }
            None => assert_eq!(candidate, wk.number),
        }
    }

    for invalid in ["no", "\u{0085}"] {
        let invalid = interner.intern_literal(LiteralValue::String(invalid.to_owned()));
        let mut candidates = Candidates::default();
        infer_from_types_for_constrained_conditional(
            &mut interner,
            invalid,
            pattern,
            &constraints,
            &mut candidates,
        );
        assert!(candidates.is_empty());
    }
}

#[test]
fn constrained_template_infer_preserves_matching_composite_constraints() {
    use crate::types::repr::TemplateType;
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let parameter_id = TypeParamId(0);
    let parameter = interner.intern_type_param(parameter_id, "N");
    let pattern = interner.intern_template(TemplateType {
        texts: vec![String::new(), String::new()],
        holes: vec![parameter],
    });
    let one = interner.intern_literal(LiteralValue::Number(1.0));
    let two = interner.intern_literal(LiteralValue::Number(2.0));
    let numeric_literals = interner.union(vec![one, two]);
    let auto = interner.intern_literal(LiteralValue::String("auto".to_owned()));
    let number_or_auto = interner.union(vec![wk.number, auto]);
    let boolean_true = interner.intern_literal(LiteralValue::Boolean(true));
    let empty_object = interner.intern_object(ObjectType::default());
    let number_and_empty = interner.intersection(vec![wk.number, empty_object]);

    for (source, constraint, expected) in [
        ("1", numeric_literals, Some(one)),
        ("3", numeric_literals, None),
        ("1", number_or_auto, Some(one)),
        ("01", number_or_auto, Some(number_or_auto)),
        ("auto", number_or_auto, Some(auto)),
        ("true", wk.boolean, Some(boolean_true)),
        ("1", number_and_empty, Some(number_and_empty)),
        ("01", number_and_empty, Some(number_and_empty)),
    ] {
        let source = interner.intern_literal(LiteralValue::String(source.to_owned()));
        let constraints = FxHashMap::from_iter([(parameter_id, constraint)]);
        let mut candidates = Candidates::default();
        infer_from_types_for_constrained_conditional(
            &mut interner,
            source,
            pattern,
            &constraints,
            &mut candidates,
        );
        assert_eq!(
            candidates
                .get(&parameter_id)
                .and_then(|candidates| candidates.first())
                .copied(),
            expected,
            "composite constrained capture {source:?}"
        );
    }
}

#[test]
fn conditional_tuple_rest_infer_captures_middle_as_tuple() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let r = interner.intern_type_param(TypeParamId(0), "R");
    let source = interner.intern_tuple(vec![wk.string, wk.number, wk.boolean]);
    let target = interner.intern_tuple_type(TupleType::with_rest(
        vec![wk.unknown],
        TupleRestType::new(1, r),
    ));
    let expected = interner.intern_tuple(vec![wk.number, wk.boolean]);

    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(&mut interner, source, target, &mut candidates);

    assert_eq!(
        candidates.get(&TypeParamId(0)).map(|c| c.as_slice()),
        Some(&[expected][..]),
        "`[unknown, ...infer R]` captures the remaining tuple segment"
    );
}

#[test]
fn conditional_function_rest_infer_captures_parameter_tuple() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let a = interner.intern_type_param(TypeParamId(0), "A");
    let source = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![
            ParameterType::required("x", wk.string),
            ParameterType::required("y", wk.number),
        ],
        ret: wk.boolean,
    });
    let target = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::rest("args", a)],
        ret: wk.unknown,
    });
    let expected = interner.intern_tuple(vec![wk.string, wk.number]);

    let mut candidates = Candidates::default();
    infer_from_types_for_conditional(&mut interner, source, target, &mut candidates);

    assert_eq!(
        candidates.get(&TypeParamId(0)).map(|c| c.as_slice()),
        Some(&[expected][..]),
        "`(...args: infer A)` captures fixed parameters as a tuple"
    );
}

#[test]
fn call_site_rest_array_infers_from_each_variadic_argument() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(0), "T");
    let t_array = interner.intern_array(t);
    let mut next_type_param = 1;

    let map = infer_type_arguments_from_params(
        &mut interner,
        &mut next_type_param,
        &[TypeParamId(0)],
        &[ParameterType::rest("args", t_array)],
        &[wk.number, wk.string],
        &[],
    );

    assert_eq!(
        map.get(&TypeParamId(0)).copied(),
        Some(wk.number),
        "incompatible rest arguments fix to number so the string replay can fail"
    );
}

/// A self-referential argument/parameter pair terminates (cycle guard): a
/// recursive nominal object matched against itself does not loop.
#[test]
fn self_referential_types_terminate() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    // A recursive nominal `List { head: number; tail: List | null }`.
    let list = interner.reserve_object();
    let list_or_null = interner.union(vec![list, wk.null]);
    interner.fill_object(
        list,
        ObjectType {
            properties: vec![prop("head", wk.number), prop("tail", list_or_null)],
            ..Default::default()
        },
    );

    // Matching `list` against itself must terminate; it has no type parameter,
    // so it infers nothing.
    let mut next_type_param = 0;
    let map = infer_type_arguments(
        &mut interner,
        &mut next_type_param,
        &[],
        &[list],
        &[list],
        &[],
    );
    assert!(
        map.is_empty(),
        "no type params → empty map, and no infinite loop"
    );
}

fn fix_declared_params(
    interner: &mut Interner,
    type_params: &[GenericTypeParam],
    fixed: FxHashMap<TypeParamId, TypeId>,
) -> FixedSignatureParams {
    let published = PublishedClasses::empty();
    let mut queries = SemanticQueryState::default();
    let mut next_type_param = 1_000;
    match fix_signature_params(
        interner,
        &mut next_type_param,
        &published,
        &mut queries,
        type_params,
        fixed,
        &FxHashSet::default(),
    ) {
        DemandOutcome::Ready(fixed) => fixed,
        DemandOutcome::Exhausted(exhaustion) => {
            panic!("fixing unexpectedly exhausted: {exhaustion:?}")
        }
    }
}

/// `<U extends T, T extends string>` with `U = "child"`, `T = 1`: `T` fixes first, so `U`
/// is checked against the clamped `string` instead of the raw candidate `1`.
#[test]
fn reverse_dependent_constraint_fixes_the_mentioned_parameter_first() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let (u, t) = (TypeParamId(0), TypeParamId(1));
    let t_ty = interner.intern_type_param(t, "T");
    let child = interner.intern_literal(LiteralValue::String("child".into()));
    let one = interner.intern_literal(LiteralValue::Number(1.0));
    let type_params = [
        GenericTypeParam {
            id: u,
            constraint: Some(t_ty),
            default: None,
        },
        GenericTypeParam {
            id: t,
            constraint: Some(wk.string),
            default: None,
        },
    ];

    let fixed = fix_declared_params(
        &mut interner,
        &type_params,
        FxHashMap::from_iter([(u, child), (t, one)]),
    );

    assert_eq!(fixed.arguments.get(&t).copied(), Some(wk.string));
    assert_eq!(fixed.arguments.get(&u).copied(), Some(child));
    assert_eq!(fixed.constraint_violations, FxHashSet::from_iter([t]));
}

/// `<T = U, U extends string>` without candidates: the default demands `U` before `T`.
#[test]
fn default_mentioning_a_later_parameter_observes_its_final_binding() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let (t, u) = (TypeParamId(0), TypeParamId(1));
    let u_ty = interner.intern_type_param(u, "U");
    let type_params = [
        GenericTypeParam {
            id: t,
            constraint: None,
            default: Some(u_ty),
        },
        GenericTypeParam {
            id: u,
            constraint: Some(wk.string),
            default: None,
        },
    ];

    let fixed = fix_declared_params(&mut interner, &type_params, FxHashMap::default());

    assert_eq!(fixed.arguments.get(&t).copied(), Some(wk.string));
    assert_eq!(fixed.arguments.get(&u).copied(), Some(wk.string));
    assert!(fixed.constraint_violations.is_empty());
}

/// `<T extends { next: T }>` without a candidate fixes once to the constraint with one
/// symbolic layer; no budget exhaustion.
#[test]
fn self_cyclic_constraint_without_candidate_keeps_one_symbolic_layer() {
    let mut interner = Interner::with_intrinsics();
    let t = TypeParamId(0);
    let t_ty = interner.intern_type_param(t, "T");
    let constraint = interner.reserve_object();
    interner.fill_object(
        constraint,
        ObjectType {
            properties: vec![prop("next", t_ty)],
            ..Default::default()
        },
    );
    let type_params = [GenericTypeParam {
        id: t,
        constraint: Some(constraint),
        default: None,
    }];

    let fixed = fix_declared_params(&mut interner, &type_params, FxHashMap::default());

    assert_eq!(fixed.arguments.get(&t).copied(), Some(constraint));
    let next = interner
        .store()
        .object_type(constraint)
        .map(|object| object.properties[0].ty);
    assert_eq!(
        next,
        Some(t_ty),
        "the fallback keeps `T` symbolic under `next`"
    );
    assert!(fixed.constraint_violations.is_empty());
}

/// `<B extends { c: C }, C extends { b: B }>` with rejected `number` candidates: `C` fixes
/// against the raw `B`, `B` against the fixed `C`; both violations are retained.
#[test]
fn mutual_cyclic_constraints_with_rejected_candidates_fix_without_exhaustion() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let (b, c) = (TypeParamId(0), TypeParamId(1));
    let b_ty = interner.intern_type_param(b, "B");
    let c_ty = interner.intern_type_param(c, "C");
    let has_c = interner.reserve_object();
    interner.fill_object(
        has_c,
        ObjectType {
            properties: vec![prop("c", c_ty)],
            ..Default::default()
        },
    );
    let has_b = interner.reserve_object();
    interner.fill_object(
        has_b,
        ObjectType {
            properties: vec![prop("b", b_ty)],
            ..Default::default()
        },
    );
    let type_params = [
        GenericTypeParam {
            id: b,
            constraint: Some(has_c),
            default: None,
        },
        GenericTypeParam {
            id: c,
            constraint: Some(has_b),
            default: None,
        },
    ];

    let fixed = fix_declared_params(
        &mut interner,
        &type_params,
        FxHashMap::from_iter([(b, wk.number), (c, wk.number)]),
    );

    assert_eq!(fixed.constraint_violations, FxHashSet::from_iter([b, c]));
    let first_property = |ty: TypeId| {
        interner
            .store()
            .object_type(ty)
            .map(|object| object.properties[0].ty)
    };
    let fixed_c = fixed.arguments.get(&c).copied().unwrap_or(wk.never);
    let fixed_b = fixed.arguments.get(&b).copied().unwrap_or(wk.never);
    assert_eq!(
        first_property(fixed_c),
        Some(wk.number),
        "`C = {{ b: number }}`"
    );
    assert_eq!(
        first_property(fixed_b),
        Some(fixed_c),
        "`B = {{ c: C }}` over the fixed `C`"
    );

    let output = crate::check::test_support::check_source(
        r#"
declare const seeds: [number, number];
declare function graph<B extends { c: C }, C extends { b: B }>(values: [B, C]): [B, C];
const result = graph(seeds);
"#,
    );
    assert!(output.parse_errors.is_empty(), "{:?}", output.parse_errors);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>(),
        ["TK2345"]
    );
    assert!(output.incomplete.is_empty(), "{:?}", output.incomplete);
}

fn fix_present_params(
    interner: &mut Interner,
    type_params: &[GenericTypeParam],
    fixed: FxHashMap<TypeParamId, TypeId>,
    resolve_defaults: bool,
) -> Vec<(TypeParamId, TypeId)> {
    let published = PublishedClasses::empty();
    let mut queries = SemanticQueryState::default();
    let mut next_type_param = 1_000;
    match fix_present_signature_params_attempt(
        interner,
        &mut next_type_param,
        &published,
        &mut queries,
        type_params,
        fixed,
        PresentFixingPolicy {
            fresh_exempt: &FxHashSet::default(),
            resolve_defaults,
        },
    ) {
        SemanticQueryAttempt::Complete(DemandOutcome::Ready(ordered)) => ordered,
        SemanticQueryAttempt::Complete(DemandOutcome::Exhausted(exhaustion)) => {
            panic!("partial fixing unexpectedly exhausted: {exhaustion:?}")
        }
        SemanticQueryAttempt::NeedsNativeIdentities(_) => {
            unreachable!("native identity demands require the project-local Pass boundary")
        }
    }
}

/// `<T extends { a: string }, U = T>` with only `T` inferred: the contextual preview resolves
/// `U` through the fixed `T`, so a callback parameter typed `U` sees `{ a: string }`.
#[test]
fn contextual_preview_resolves_a_candidate_less_default_from_fixed_binders() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let (t, u) = (TypeParamId(0), TypeParamId(1));
    let t_ty = interner.intern_type_param(t, "T");
    let shape = interner.reserve_object();
    interner.fill_object(
        shape,
        ObjectType {
            properties: vec![prop("a", wk.string)],
            ..Default::default()
        },
    );
    let type_params = [
        GenericTypeParam {
            id: t,
            constraint: Some(shape),
            default: None,
        },
        GenericTypeParam {
            id: u,
            constraint: None,
            default: Some(t_ty),
        },
    ];

    let preview = fix_present_params(
        &mut interner,
        &type_params,
        FxHashMap::from_iter([(t, shape)]),
        true,
    );
    assert_eq!(preview, vec![(t, shape), (u, shape)]);

    let application = fix_present_params(
        &mut interner,
        &type_params,
        FxHashMap::from_iter([(t, shape)]),
        false,
    );
    assert_eq!(
        application,
        vec![(t, shape)],
        "non-preview callers keep defaults unresolved"
    );

    let clean = crate::check::test_support::check_source(
        r#"
declare function pd<T extends { a: string }, U = T>(t: T, cb: (u: U) => void): void;
pd({ a: "x" }, (u) => { const ok: string = u.a; });
"#,
    );
    assert!(clean.parse_errors.is_empty(), "{:?}", clean.parse_errors);
    assert!(clean.diagnostics.is_empty(), "{:?}", clean.diagnostics);
    assert!(clean.incomplete.is_empty(), "{:?}", clean.incomplete);

    let source = r#"
declare function pd<T extends { a: string }, U = T>(t: T, cb: (u: U) => void): void;
pd({ a: "x" }, (u) => { const bad: number = u.a; });
"#;
    let wrong = crate::check::test_support::check_source(source);
    assert_eq!(
        wrong
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code.as_str(), &source[diagnostic.span.range()]))
            .collect::<Vec<_>>(),
        [("TK2322", "bad")]
    );
    assert!(wrong.incomplete.is_empty(), "{:?}", wrong.incomplete);
}

/// `<T, U = T>` with no candidate for `T`: the default mentions an unfixed binder, so `U`
/// stays symbolic (absent from the partial result) instead of leaking a raw `T`.
#[test]
fn contextual_preview_keeps_a_default_over_unfixed_binders_symbolic() {
    let mut interner = Interner::with_intrinsics();
    let (t, u) = (TypeParamId(0), TypeParamId(1));
    let t_ty = interner.intern_type_param(t, "T");
    let type_params = [
        GenericTypeParam {
            id: t,
            constraint: None,
            default: None,
        },
        GenericTypeParam {
            id: u,
            constraint: None,
            default: Some(t_ty),
        },
    ];

    let preview = fix_present_params(&mut interner, &type_params, FxHashMap::default(), true);
    assert!(preview.is_empty(), "{preview:?}");
}

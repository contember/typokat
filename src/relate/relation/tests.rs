use super::*;
use crate::class_semantics::Exhaustion;
use crate::types::repr::{
    ClassId, FunctionType, LiteralValue, ObjectType, ParameterType, PropertyType, Visibility,
};
use crate::types::Interner;
use std::time::Instant;

fn prop(name: &str, ty: TypeId) -> PropertyType {
    PropertyType::public(name, ty)
}

#[test]
fn typed_relation_outcome_requires_exhaustive_matching() {
    fn classify(value: RelationOutcome) -> u8 {
        match value {
            RelationOutcome::Yes => 0,
            RelationOutcome::No(_) => 1,
            RelationOutcome::Exhausted(_) => 2,
        }
    }

    assert_eq!(classify(RelationOutcome::Yes), 0);
    let failure = std::sync::Arc::new(ReasonChain::leaf(TypeId(0), TypeId(1)));
    assert_eq!(classify(RelationOutcome::No(failure)), 1);
    assert_eq!(
        classify(RelationOutcome::Exhausted(
            Exhaustion::ClassProjectionBudget
        )),
        2
    );
}

/// Build an optional member `name?: ty` (M21). The caller passes the *effective*
/// type — i.e. already `T | undefined`, as it is unioned in at lowering — exactly
/// as the relation engine sees it (it never interns `| undefined` itself).
fn optional_prop(name: &str, ty: TypeId) -> PropertyType {
    PropertyType {
        optional: true,
        ..PropertyType::public(name, ty)
    }
}

/// Build a member `name: ty` with an explicit visibility + declaring class
/// (M13), for the nominal-relation tests.
fn nominal_prop(
    name: &str,
    ty: TypeId,
    visibility: Visibility,
    declaring_class: Option<ClassId>,
) -> PropertyType {
    PropertyType {
        name: name.to_string(),
        ty,
        write_ty: None,
        optional: false,
        visibility,
        declaring_class,
        readonly: false,
        is_accessor: false,
    }
}

fn measured_relation_pairs(
    interner: &mut Interner,
    count: usize,
    width: usize,
) -> Vec<(TypeId, TypeId)> {
    let number = interner.well_known().number;
    (0..count)
        .map(|group| {
            let properties: Vec<_> = (0..width)
                .map(|index| prop(&format!("g{group:06}_p{index:02}"), number))
                .collect();
            let source = interner.reserve_object();
            interner.fill_object(
                source,
                ObjectType {
                    properties: properties.clone(),
                    ..Default::default()
                },
            );
            let target = interner.reserve_object();
            interner.fill_object(
                target,
                ObjectType {
                    properties,
                    ..Default::default()
                },
            );
            (source, target)
        })
        .collect()
}

#[test]
fn measure_relation_counts_actual_empty_context_key_and_property_scans() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let pairs = measured_relation_pairs(&mut interner, 2, 3);
    reset_relation_measure();
    let mut relater = Relater::new(interner.store(), wk);
    for (source, target) in pairs {
        assert!(relater.is_assignable(source, target).is_yes());
    }
    assert_eq!(
        relation_measure(),
        RelationMeasure {
            stack_key_builds: 2,
            empty_context_stack_keys: 2,
            object_target_properties: 6,
            object_source_property_comparisons: 6,
            ..RelationMeasure::default()
        }
    );
}

fn measure_wide_generic_signature_environment(width: usize) -> RelationMeasure {
    use crate::types::repr::{GenericTypeParam, TypeParamId};

    fn signature(interner: &mut Interner, parameter: TypeParamId, width: usize) -> TypeId {
        let parameter_type = interner.intern_type_param(parameter, "T");
        let payload = interner.intern_object(ObjectType {
            properties: vec![prop("value", parameter_type)],
            ..Default::default()
        });
        let input = interner.intern_object(ObjectType {
            properties: (0..width)
                .map(|index| prop(&format!("event{index:04}"), payload))
                .collect(),
            ..Default::default()
        });
        interner.intern_function(FunctionType {
            type_params: vec![GenericTypeParam {
                id: parameter,
                constraint: None,
                default: None,
            }],
            receiver: None,
            params: vec![ParameterType::required("events", input)],
            ret: interner.well_known().void,
        })
    }

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let source = signature(&mut interner, TypeParamId(91_001), width);
    let target = signature(&mut interner, TypeParamId(91_002), width);
    reset_relation_measure();
    let mut relater = Relater::new(interner.store(), wk);
    assert!(relater.is_assignable(source, target).is_yes());
    assert!(relater.is_assignable(target, source).is_yes());
    relation_measure()
}

#[test]
fn wide_generic_signature_reuses_its_effective_binder_environment() {
    const SMALL_WIDTH: usize = 16;
    const LARGE_WIDTH: usize = 256;

    let small = measure_wide_generic_signature_environment(SMALL_WIDTH);
    let large = measure_wide_generic_signature_environment(LARGE_WIDTH);
    assert!(
        large.object_target_properties >= 8 * small.object_target_properties,
        "witness did not scale outer structural work: small={small:?}, large={large:?}"
    );
    assert!(
        large.flattened_environment_entries <= small.flattened_environment_entries + 16,
        "binder environments were rebuilt per relation frame: small={small:?}, large={large:?}"
    );
    assert!(
        large.environment_sort_items <= small.environment_sort_items + 8,
        "binder environments were re-sorted per relation frame: small={small:?}, large={large:?}"
    );
}

#[test]
fn measure_relation_keeps_first_target_failure_order() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let source = interner.reserve_object();
    interner.fill_object(
        source,
        ObjectType {
            properties: vec![prop("a", wk.number), prop("b", wk.number)],
            ..Default::default()
        },
    );
    let target = interner.reserve_object();
    interner.fill_object(
        target,
        ObjectType {
            properties: vec![prop("a", wk.string), prop("b", wk.string)],
            ..Default::default()
        },
    );
    reset_relation_measure();
    let result = Relater::new(interner.store(), wk).is_assignable(source, target);
    match result {
        Relation::No(reason) => match reason.head {
            Reason::Property { name, .. } => assert_eq!(name, "a"),
            other => panic!("expected first property mismatch, got {other:?}"),
        },
        Relation::Yes => panic!("the mismatched first target property must fail"),
    }
    assert_eq!(relation_measure().object_target_properties, 1);
    assert_eq!(relation_measure().object_source_property_comparisons, 1);
}

#[test]
fn ordered_object_cursor_preserves_width_and_first_failure_semantics() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let num_or_undef = interner.union(vec![wk.number, wk.undefined]);

    // Source-only names before, between, and after target names remain width-only.
    let extras = interner.intern_object(ObjectType {
        properties: vec![
            prop("a", wk.number),
            prop("b", wk.number),
            prop("c", wk.number),
            prop("d", wk.number),
            prop("e", wk.number),
        ],
        ..Default::default()
    });
    let sparse_target = interner.intern_object(ObjectType {
        properties: vec![prop("b", wk.number), prop("d", wk.number)],
        ..Default::default()
    });

    let early_missing_source = interner.intern_object(ObjectType {
        properties: vec![prop("b", wk.number), prop("d", wk.number)],
        ..Default::default()
    });
    let early_missing_target = interner.intern_object(ObjectType {
        properties: vec![
            prop("a", wk.number),
            prop("b", wk.number),
            prop("d", wk.number),
        ],
        ..Default::default()
    });
    let late_missing_source = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number), prop("b", wk.number)],
        ..Default::default()
    });
    let late_missing_target = interner.intern_object(ObjectType {
        properties: vec![
            prop("a", wk.number),
            prop("b", wk.number),
            prop("c", wk.number),
        ],
        ..Default::default()
    });

    let missing_source = interner.intern_object(ObjectType {
        properties: vec![
            prop("a", wk.number),
            prop("b", wk.number),
            prop("d", wk.number),
            prop("e", wk.number),
        ],
        ..Default::default()
    });
    let missing_target = interner.intern_object(ObjectType {
        properties: vec![
            prop("b", wk.number),
            prop("c", wk.number),
            prop("d", wk.number),
        ],
        ..Default::default()
    });

    let mismatch_source = interner.intern_object(ObjectType {
        properties: vec![
            prop("a", wk.number),
            prop("b", wk.number),
            prop("c", wk.number),
            prop("d", wk.string),
            prop("e", wk.number),
        ],
        ..Default::default()
    });
    let mismatch_target = interner.intern_object(ObjectType {
        properties: vec![prop("b", wk.number), prop("d", wk.number)],
        ..Default::default()
    });
    let early_mismatch_source = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.string), prop("b", wk.number)],
        ..Default::default()
    });
    let early_mismatch_target = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number), prop("b", wk.number)],
        ..Default::default()
    });

    let optional_source = interner.intern_object(ObjectType {
        properties: vec![optional_prop("b", num_or_undef)],
        ..Default::default()
    });
    let required_target = interner.intern_object(ObjectType {
        properties: vec![prop("b", wk.number)],
        ..Default::default()
    });
    let public_source = interner.intern_object(ObjectType {
        properties: vec![prop("b", wk.number)],
        ..Default::default()
    });
    let private_target = interner.intern_object(ObjectType {
        properties: vec![nominal_prop(
            "b",
            wk.number,
            Visibility::Private,
            Some(ClassId(0)),
        )],
        ..Default::default()
    });

    reset_relation_measure();
    let mut relater = Relater::new(interner.store(), wk);
    assert!(relater.is_assignable(extras, sparse_target).is_yes());
    assert_eq!(relation_measure().object_target_properties, 2);
    assert_eq!(relation_measure().object_source_property_comparisons, 4);

    reset_relation_measure();
    match relater.is_assignable(early_missing_source, early_missing_target) {
        Relation::No(reason) => match reason.head {
            Reason::MissingProperty { name, .. } => assert_eq!(name, "a"),
            other => panic!("expected the first missing target property, got {other:?}"),
        },
        Relation::Yes => panic!("the first target property must be missing"),
    }
    assert_eq!(relation_measure().object_target_properties, 1);
    assert_eq!(relation_measure().object_source_property_comparisons, 1);

    reset_relation_measure();
    match relater.is_assignable(late_missing_source, late_missing_target) {
        Relation::No(reason) => match reason.head {
            Reason::MissingProperty { name, .. } => assert_eq!(name, "c"),
            other => panic!("expected the last missing target property, got {other:?}"),
        },
        Relation::Yes => panic!("the final target property must be missing"),
    }
    assert_eq!(relation_measure().object_target_properties, 3);
    assert_eq!(relation_measure().object_source_property_comparisons, 2);

    reset_relation_measure();
    match relater.is_assignable(missing_source, missing_target) {
        Relation::No(reason) => match reason.head {
            Reason::MissingProperty { name, .. } => assert_eq!(name, "c"),
            other => panic!("expected the first missing target property, got {other:?}"),
        },
        Relation::Yes => panic!("the middle target property must be missing"),
    }
    assert_eq!(relation_measure().object_target_properties, 2);
    assert_eq!(relation_measure().object_source_property_comparisons, 3);

    reset_relation_measure();
    match relater.is_assignable(early_mismatch_source, early_mismatch_target) {
        Relation::No(reason) => match reason.head {
            Reason::Property { name, .. } => assert_eq!(name, "a"),
            other => panic!("expected the first target mismatch, got {other:?}"),
        },
        Relation::Yes => panic!("the first target property must mismatch"),
    }
    assert_eq!(relation_measure().object_target_properties, 1);
    assert_eq!(relation_measure().object_source_property_comparisons, 1);

    reset_relation_measure();
    match relater.is_assignable(mismatch_source, mismatch_target) {
        Relation::No(reason) => match reason.head {
            Reason::Property { name, .. } => assert_eq!(name, "d"),
            other => panic!("expected the later target mismatch, got {other:?}"),
        },
        Relation::Yes => panic!("the later target property must mismatch"),
    }
    assert_eq!(relation_measure().object_target_properties, 2);
    assert_eq!(relation_measure().object_source_property_comparisons, 4);

    assert!(matches!(
        relater.is_assignable(optional_source, required_target),
        Relation::No(_)
    ));
    assert!(matches!(
        relater.is_assignable(public_source, private_target),
        Relation::No(_)
    ));
}

#[test]
#[ignore = "WU4 release measurement; run explicitly with --ignored --nocapture"]
fn measure_relation_hotpaths_release() {
    const WIDTH: usize = 8;
    for count in [10_000, 100_000] {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let pairs = measured_relation_pairs(&mut interner, count, WIDTH);
        reset_relation_measure();
        let started = Instant::now();
        let mut relater = Relater::new(interner.store(), wk);
        for (source, target) in pairs {
            assert!(relater.is_assignable(source, target).is_yes());
        }
        let elapsed = started.elapsed();
        let measure = relation_measure();
        assert_eq!(measure.stack_key_builds, count as u64);
        assert_eq!(measure.empty_context_stack_keys, count as u64);
        assert_eq!(measure.object_target_properties, (count * WIDTH) as u64);
        assert_eq!(
            measure.object_source_property_comparisons,
            (count * WIDTH) as u64
        );
        println!(
            "WU4 relation count={count} width={WIDTH} elapsed_ms={} counters={measure:?}",
            elapsed.as_millis()
        );
    }
}

/// The M2 object structural rule: width (extra src props ok), depth (prop
/// types checked, recursing), and the precise missing-vs-mismatch reason.
#[test]
fn object_width_depth_and_reason_kinds() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    // { a: number; b: string }
    let ab = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number), prop("b", wk.string)],
        ..Default::default()
    });
    // { a: number } — a width-narrower target.
    let a_only = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number)],
        ..Default::default()
    });
    // { a: string } — same key, incompatible type.
    let a_str = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.string)],
        ..Default::default()
    });

    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    // Width: { a; b } is assignable to { a } (extra `b` ignored).
    assert!(rel.is_assignable(ab, a_only).is_yes());
    // Exact identity short-circuits.
    assert!(rel.is_assignable(ab, ab).is_yes());

    // Missing required property: { a } is NOT assignable to { a; b }.
    match rel.is_assignable(a_only, ab) {
        Relation::No(chain) => match chain.head() {
            Reason::MissingProperty { name, .. } => assert_eq!(name, "b"),
            other => panic!("expected MissingProperty, got {other:?}"),
        },
        Relation::Yes => panic!("expected a missing-property failure"),
    }

    // Depth mismatch: { a: number } is NOT assignable to { a: string }.
    match rel.is_assignable(a_only, a_str) {
        Relation::No(chain) => match chain.head() {
            Reason::Property { name, because, .. } => {
                assert_eq!(name, "a");
                assert!(matches!(**because, Reason::Leaf { .. }));
            }
            other => panic!("expected Property, got {other:?}"),
        },
        Relation::Yes => panic!("expected a depth mismatch failure"),
    }
}

/// M21 — the optional-property soundness core. With an optional member's
/// effective type unioned to `T | undefined` at lowering and `optional: true`,
/// the relation must: (1) let a REQUIRED source satisfy an OPTIONAL target — the
/// optional target may be absent in the source, so its absence is allowed; and
/// (2) reject an OPTIONAL source against a REQUIRED target — the source's value
/// is `T | undefined`, whose `undefined` arm fails the required `T`. Pinning both
/// directions guards against a dropped error (the worst outcome for a checker).
#[test]
fn optional_target_absent_ok_optional_source_to_required_fails() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    // The effective type of an optional `a?: number` — `number | undefined`.
    let num_or_undef = interner.union(vec![wk.number, wk.undefined]);

    // `{ a: number }` — a required member.
    let required = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number)],
        ..Default::default()
    });
    // `{ a?: number }` — an optional member (effective type `number | undefined`).
    let optional = interner.intern_object(ObjectType {
        properties: vec![optional_prop("a", num_or_undef)],
        ..Default::default()
    });
    // `{}` — the empty object (`a` absent).
    let empty = interner.intern_object(ObjectType::default());

    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    // (1a) A required `a` satisfies an optional `a` (present source, related
    // against `number | undefined` via the union-target logic).
    assert!(
        rel.is_assignable(required, optional).is_yes(),
        "a required `a` must satisfy an optional `a`"
    );
    // (1b) An ABSENT optional target is allowed — `{}` is assignable to `{ a? }`.
    assert!(
        rel.is_assignable(empty, optional).is_yes(),
        "an absent optional target property must be allowed"
    );

    // (2) An optional `a` does NOT satisfy a required `a` — presence is independent
    // of the value type: the source may OMIT `a` entirely, which the required target
    // forbids. This is an object-level structural failure (a `Leaf`), NOT a
    // value-depth `Property` failure — the gate fires before the value relation, so
    // it holds even when the value types would relate (a required `a: number |
    // undefined` is likewise rejected). Maps to `TK2322` (assignment) / `TK2345`
    // (argument), like the nominal-origin leaf.
    match rel.is_assignable(optional, required) {
        Relation::No(chain) => {
            assert!(
                matches!(chain.head(), Reason::Leaf { .. }),
                "expected an object-level Leaf failure, got {:?}",
                chain.head()
            );
            // The root pins the two object types.
            assert_eq!(chain.root(), (optional, required));
        }
        Relation::Yes => panic!("an optional source must NOT satisfy a required target"),
    }
}

#[test]
fn type_parameter_intersection_constraint_satisfies_its_union_conjunct() {
    use crate::types::repr::TypeParamId;

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let target = interner.union(vec![wk.number, wk.string]);
    let refinement = interner.intern_object(ObjectType {
        properties: vec![optional_prop("marker", wk.undefined)],
        ..Default::default()
    });
    let constraint = interner.intersection(vec![target, refinement]);
    let parameter_id = TypeParamId(40_001);
    let parameter = interner.intern_type_param(parameter_id, "T");
    assert!(interner.set_type_param_constraint(parameter_id, constraint));

    let store = interner.store();
    let mut rel = Relater::new(store, wk);
    assert!(
        rel.is_assignable(parameter, target).is_yes(),
        "T extends (number | string) & Refinement must satisfy number | string"
    );
}

/// Explicit receivers are non-positional but contravariant when both sides
/// declare one; a receiverless signature remains compatible in either direction.
#[test]
fn function_receivers_are_contravariant_and_optional() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let wide = interner.intern_object(ObjectType {
        properties: vec![prop("tag", wk.string)],
        ..Default::default()
    });
    let narrow_tag = interner.intern_literal(LiteralValue::String("narrow".to_string()));
    let narrow = interner.intern_object(ObjectType {
        properties: vec![prop("tag", narrow_tag)],
        ..Default::default()
    });
    let with_wide = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: Some(wide),
        params: vec![ParameterType::required("value", wk.number)],
        ret: wk.void,
    });
    let with_narrow = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: Some(narrow),
        params: vec![ParameterType::required("value", wk.number)],
        ret: wk.void,
    });
    let without_receiver = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required("value", wk.number)],
        ret: wk.void,
    });

    let store = interner.store();
    let mut rel = Relater::new(store, wk);
    assert!(rel.is_assignable(with_wide, with_narrow).is_yes());
    assert!(matches!(
        rel.is_assignable(with_narrow, with_wide),
        Relation::No(_)
    ));
    assert!(rel.is_assignable(with_wide, without_receiver).is_yes());
    assert!(rel.is_assignable(without_receiver, with_wide).is_yes());
}

/// M13 — nominal class typing via a `private`/`protected` member. A
/// `private`/`protected` **target** member breaks pure structural
/// assignability: a structurally-identical public object is NOT assignable, and
/// a same-named non-public member from a *different* declaring class is NOT
/// assignable; only the class's own instances (same interned id) are. This pins
/// the soundness-critical rule (a non-matching origin must FAIL the relation).
#[test]
fn nominal_private_member_breaks_structural_assignability() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let secret = ClassId(0);
    let other = ClassId(1);

    // `class Secret { private x: number }` (its instance type, nominal object).
    let secret_ty = interner.intern_object(ObjectType {
        properties: vec![nominal_prop(
            "x",
            wk.number,
            Visibility::Private,
            Some(secret),
        )],
        ..Default::default()
    });
    // A structurally identical *public* object literal `{ x: number }`.
    let public_obj = interner.intern_object(ObjectType {
        properties: vec![prop("x", wk.number)],
        ..Default::default()
    });
    // `class Other { private x: number }` — same shape, DIFFERENT origin.
    let other_ty = interner.intern_object(ObjectType {
        properties: vec![nominal_prop(
            "x",
            wk.number,
            Visibility::Private,
            Some(other),
        )],
        ..Default::default()
    });

    // The three are distinct interned ids (origin/visibility are part of
    // identity), so the relation cache keys them apart.
    assert_ne!(
        secret_ty, public_obj,
        "private member ⇒ distinct from public"
    );
    assert_ne!(secret_ty, other_ty, "different declaring class ⇒ distinct");

    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    // Same class (same id): assignable via the identity fast path.
    assert!(
        rel.is_assignable(secret_ty, secret_ty).is_yes(),
        "a class's own instance type is assignable to itself"
    );

    // Public `{ x: number }` is NOT assignable to `Secret` (the private member
    // has no public counterpart) — an object-level `Leaf` failure (the member
    // type is fine; only the visibility/origin differs).
    match rel.is_assignable(public_obj, secret_ty) {
        Relation::No(chain) => {
            assert!(
                matches!(chain.head(), Reason::Leaf { .. }),
                "expected an object-level Leaf failure, got {:?}",
                chain.head()
            );
            // The root pins the two object types.
            assert_eq!(chain.root(), (public_obj, secret_ty));
        }
        Relation::Yes => {
            panic!("a public object must NOT be assignable to a private-member class")
        }
    }

    // `Other` (different origin) is NOT assignable to `Secret`.
    assert!(
        !rel.is_assignable(other_ty, secret_ty).is_yes(),
        "a different class's private member must not satisfy Secret's"
    );

    // Symmetry: `Secret` is also not assignable to `Other` (origin differs).
    assert!(
        !rel.is_assignable(secret_ty, other_ty).is_yes(),
        "Secret's private member must not satisfy Other's"
    );
}

/// M13 — a `protected` target member is nominal exactly like `private`: a
/// structurally-identical public object is not assignable, while the class's own
/// instance is. Separated from the `private` test so each uses a clean interner.
#[test]
fn nominal_protected_member_breaks_structural_assignability() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let owner = ClassId(0);

    let prot_ty = interner.intern_object(ObjectType {
        properties: vec![nominal_prop(
            "owner",
            wk.string,
            Visibility::Protected,
            Some(owner),
        )],
        ..Default::default()
    });
    let public_obj = interner.intern_object(ObjectType {
        properties: vec![prop("owner", wk.string)],
        ..Default::default()
    });

    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    assert!(
        rel.is_assignable(prot_ty, prot_ty).is_yes(),
        "own instance ok"
    );
    assert!(
        !rel.is_assignable(public_obj, prot_ty).is_yes(),
        "a public object must NOT be assignable to a protected-member class"
    );
}

/// WU3 — the nominal-origin rule must hold when the SOURCE is an intersection (the
/// merged-source relation path). A purely structural intersection cannot satisfy a
/// private-member target (unsound accept if it could); an intersection that INCLUDES the
/// nominal class does. Same-origin acceptance stays intact, and repeated/reordered
/// queries are stable (the fix adds a deterministic, assumption-free leaf rejection, so
/// the cache never becomes query-order dependent).
#[test]
fn nominal_origin_holds_through_intersection_source() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let secret = ClassId(0);

    // `class Secret { private p: number }` (its nominal instance type).
    let secret_ty = interner.intern_object(ObjectType {
        properties: vec![nominal_prop(
            "p",
            wk.number,
            Visibility::Private,
            Some(secret),
        )],
        ..Default::default()
    });
    let struct_p = interner.intern_object(ObjectType {
        properties: vec![prop("p", wk.number)],
        ..Default::default()
    });
    let struct_q = interner.intern_object(ObjectType {
        properties: vec![prop("q", wk.number)],
        ..Default::default()
    });

    // Structural `{ p: number } & { q: number }` — no nominal origin for `p`.
    let bad_inter = interner.intersection(vec![struct_p, struct_q]);
    // `Secret & { q: number }` — `Secret` contributes the matching-origin `p`.
    let good_inter = interner.intersection(vec![secret_ty, struct_q]);

    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    // The structural intersection must NOT satisfy the private-member class.
    match rel.is_assignable(bad_inter, secret_ty) {
        Relation::No(chain) => assert!(
            matches!(chain.head(), Reason::Leaf { .. }),
            "expected an object-level Leaf failure, got {:?}",
            chain.head()
        ),
        Relation::Yes => panic!("a structural intersection must not satisfy a private target"),
    }
    // An intersection that includes `Secret` itself does.
    assert!(
        rel.is_assignable(good_inter, secret_ty).is_yes(),
        "an intersection carrying the nominal class satisfies it"
    );
    // The same structural intersection IS assignable to a purely structural target.
    assert!(
        rel.is_assignable(bad_inter, struct_p).is_yes(),
        "a public structural target imposes no origin requirement"
    );

    // Stability: repeated and reordered queries give identical verdicts (no
    // query-order dependence introduced by the nominal check).
    assert!(!rel.is_assignable(bad_inter, secret_ty).is_yes());
    assert!(rel.is_assignable(good_inter, secret_ty).is_yes());

    let store = interner.store();
    let mut rel2 = Relater::new(store, wk);
    // Reverse order in a fresh relater — same results.
    assert!(rel2.is_assignable(good_inter, secret_ty).is_yes());
    assert!(!rel2.is_assignable(bad_inter, secret_ty).is_yes());
}

/// M14 — `readonly` is part of a member's structural identity (a `{ readonly x }`
/// interns to a *distinct* id from `{ x }`), but it must **NOT** affect
/// assignability: a readonly-bearing object and a mutable one relate **both ways**.
/// The relation engine deliberately ignores the flag (it gates assignment targets
/// only); this pins that it is neither added to the nominal-origin gate nor the
/// structural depth check.
#[test]
fn readonly_does_not_affect_assignability() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    // `{ readonly x: number }` and `{ x: number }`.
    let readonly_obj = interner.intern_object(ObjectType {
        properties: vec![PropertyType {
            name: "x".to_string(),
            ty: wk.number,
            write_ty: None,
            optional: false,
            visibility: Visibility::Public,
            declaring_class: None,
            readonly: true,
            is_accessor: false,
        }],
        ..Default::default()
    });
    let mutable_obj = interner.intern_object(ObjectType {
        properties: vec![prop("x", wk.number)],
        ..Default::default()
    });

    // The flag is part of identity, so the two ids differ...
    assert_ne!(
        readonly_obj, mutable_obj,
        "`readonly` is part of structural identity ⇒ distinct interned ids"
    );

    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    // ...yet they relate freely in BOTH directions (readonly ignored for relation).
    assert!(
        rel.is_assignable(readonly_obj, mutable_obj).is_yes(),
        "{{ readonly x }} must be assignable to {{ x }}"
    );
    assert!(
        rel.is_assignable(mutable_obj, readonly_obj).is_yes(),
        "{{ x }} must be assignable to {{ readonly x }}"
    );
}

#[test]
fn accessor_write_type_does_not_affect_assignability() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    let mut string_writer = prop("x", wk.number);
    string_writer.write_ty = Some(wk.string);
    let string_object = interner.intern_object(ObjectType {
        properties: vec![string_writer],
        ..Default::default()
    });

    let mut boolean_writer = prop("x", wk.number);
    boolean_writer.write_ty = Some(wk.boolean);
    let boolean_object = interner.intern_object(ObjectType {
        properties: vec![boolean_writer],
        ..Default::default()
    });

    assert_ne!(string_object, boolean_object);
    let store = interner.store();
    let mut rel = Relater::new(store, wk);
    assert!(rel.is_assignable(string_object, boolean_object).is_yes());
    assert!(rel.is_assignable(boolean_object, string_object).is_yes());
}

/// M15 — `is_accessor` mirrors `readonly`: it is part of a member's structural
/// identity (so a get-only-accessor property and a same-typed `readonly` data field
/// are distinct interned ids) but is **ignored** by the relation engine — an accessor
/// property and a plain field relate freely, both ways. This pins that the assignment
/// distinction (accessor read-only everywhere vs. field assignable in its ctor) lives
/// purely in the checker, never leaking into assignability.
#[test]
fn is_accessor_does_not_affect_assignability() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    // A get-only accessor models as `readonly: true, is_accessor: true`.
    let accessor_obj = interner.intern_object(ObjectType {
        properties: vec![PropertyType {
            name: "x".to_string(),
            ty: wk.number,
            write_ty: None,
            optional: false,
            visibility: Visibility::Public,
            declaring_class: None,
            readonly: true,
            is_accessor: true,
        }],
        ..Default::default()
    });
    // A `readonly` data field: same shape but `is_accessor: false`.
    let readonly_field_obj = interner.intern_object(ObjectType {
        properties: vec![PropertyType {
            name: "x".to_string(),
            ty: wk.number,
            write_ty: None,
            optional: false,
            visibility: Visibility::Public,
            declaring_class: None,
            readonly: true,
            is_accessor: false,
        }],
        ..Default::default()
    });
    let mutable_obj = interner.intern_object(ObjectType {
        properties: vec![prop("x", wk.number)],
        ..Default::default()
    });

    // `is_accessor` is part of identity, so the accessor object differs from the
    // same-shape `readonly` field object (and from a plain field object).
    assert_ne!(
        accessor_obj, readonly_field_obj,
        "`is_accessor` is part of structural identity ⇒ distinct interned ids"
    );

    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    // ...yet the accessor relates freely with a plain field, both directions.
    assert!(
        rel.is_assignable(accessor_obj, mutable_obj).is_yes(),
        "accessor `{{ x }}` must be assignable to field `{{ x }}`"
    );
    assert!(
        rel.is_assignable(mutable_obj, accessor_obj).is_yes(),
        "field `{{ x }}` must be assignable to accessor `{{ x }}`"
    );
}

/// Nested depth: a mismatch one level deep nests under the outer property
/// (the chain M6 renders).
#[test]
fn object_nested_depth_reason_nests() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    // inner targets: { b: number } vs source { b: string }
    let inner_num = interner.intern_object(ObjectType {
        properties: vec![prop("b", wk.number)],
        ..Default::default()
    });
    let inner_str = interner.intern_object(ObjectType {
        properties: vec![prop("b", wk.string)],
        ..Default::default()
    });
    let outer_src = interner.intern_object(ObjectType {
        properties: vec![prop("a", inner_str)],
        ..Default::default()
    });
    let outer_tgt = interner.intern_object(ObjectType {
        properties: vec![prop("a", inner_num)],
        ..Default::default()
    });

    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    match rel.is_assignable(outer_src, outer_tgt) {
        Relation::No(chain) => match chain.head() {
            Reason::Property { name, because, .. } => {
                assert_eq!(name, "a");
                // Inner reason is the property `b` mismatch.
                match &**because {
                    Reason::Property { name, .. } => assert_eq!(name, "b"),
                    other => panic!("expected nested Property, got {other:?}"),
                }
            }
            other => panic!("expected outer Property, got {other:?}"),
        },
        Relation::Yes => panic!("expected nested depth failure"),
    }
}

/// M3 function assignability: contravariant parameters, covariant return,
/// fewer source params allowed, surplus source params rejected, and `void`
/// target return accepts any source return.
#[test]
fn function_variance_arity_and_void_return() {
    use crate::types::repr::{FunctionType, ParameterType};

    fn param(name: &str, ty: TypeId) -> ParameterType {
        ParameterType::required(name, ty)
    }

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    // Reference: `(x: number) => number`.
    let num_to_num = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![param("x", wk.number)],
        ret: wk.number,
    });
    // `(x: unknown) => number`.
    let unknown_to_num = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![param("x", wk.unknown)],
        ret: wk.number,
    });
    // `(x: string) => number`.
    let str_to_num = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![param("x", wk.string)],
        ret: wk.number,
    });
    // `() => number` (FEWER params than `num_to_num`).
    let nullary_to_num = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![],
        ret: wk.number,
    });
    // `(x: number) => string` (incompatible return).
    let num_to_str = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![param("x", wk.number)],
        ret: wk.string,
    });
    // `(x: number, y: number) => number` (MORE params than `num_to_num`).
    let two_to_num = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![param("x", wk.number), param("y", wk.number)],
        ret: wk.number,
    });
    // `() => void` and `() => number` for the void-return rule.
    let nullary_to_void = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![],
        ret: wk.void,
    });
    let nullary_to_num_only = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![],
        ret: wk.number,
    });

    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    // CONTRAVARIANT params: `(x: unknown) => number` IS assignable to
    // `(x: number) => number`, because the target param `number` is
    // assignable to the source param `unknown` (tgt → src).
    assert!(
        rel.is_assignable(unknown_to_num, num_to_num).is_yes(),
        "contravariant: wider param (unknown) accepts a narrower target param (number)"
    );

    // `(x: string) => number` is NOT assignable to `(x: number) => number`:
    // the target param `number` is not assignable to the source param
    // `string`.
    match rel.is_assignable(str_to_num, num_to_num) {
        Relation::No(chain) => match chain.head() {
            Reason::Parameter { index, because, .. } => {
                assert_eq!(*index, 0);
                // The contravariant child compares `number` (tgt) → `string`
                // (src) and fails as a leaf.
                assert!(matches!(**because, Reason::Leaf { .. }));
            }
            other => panic!("expected a Parameter reason, got {other:?}"),
        },
        Relation::Yes => panic!("expected a contravariant parameter failure"),
    }

    // COVARIANT return: `(x: number) => string` is NOT assignable to
    // `(x: number) => number` — the source return `string` is not assignable
    // to the target return `number`.
    match rel.is_assignable(num_to_str, num_to_num) {
        Relation::No(chain) => match chain.head() {
            Reason::ReturnType { because, .. } => {
                assert!(matches!(**because, Reason::Leaf { .. }));
            }
            other => panic!("expected a ReturnType reason, got {other:?}"),
        },
        Relation::Yes => panic!("expected a covariant return failure"),
    }

    // FEWER source params: `() => number` IS assignable to
    // `(x: number) => number` — the source ignores the extra argument.
    assert!(
        rel.is_assignable(nullary_to_num, num_to_num).is_yes(),
        "a source with fewer parameters is assignable (extra args ignored)"
    );

    // MORE source params: `(x: number, y: number) => number` is NOT assignable
    // to `(x: number) => number` — the target cannot supply the surplus
    // parameter.
    match rel.is_assignable(two_to_num, num_to_num) {
        Relation::No(chain) => {
            assert!(matches!(chain.head(), Reason::ParameterCount { .. }));
        }
        Relation::Yes => panic!("expected a surplus-source-parameter arity failure"),
    }

    // VOID target return: `() => number` IS assignable to `() => void` — the
    // returned value is discarded.
    assert!(
        rel.is_assignable(nullary_to_num_only, nullary_to_void)
            .is_yes(),
        "a value-returning function is assignable to a void-returning function type"
    );
    // `() => void` is assignable to itself (identity), and a void source is
    // fine for a void target.
    assert!(rel.is_assignable(nullary_to_void, nullary_to_void).is_yes());

    // Identity short-circuits.
    assert!(rel.is_assignable(num_to_num, num_to_num).is_yes());
}

/// B41 — generic function binders compare by position, never declaration id.
/// The aligned type-parameter children are local to the signature comparison and
/// must not become durable raw-id cache entries.
#[test]
fn generic_function_binders_alpha_align_and_specialize_one_way() {
    use crate::types::repr::{FunctionType, GenericTypeParam, ParameterType, TypeParamId};

    fn generic_identity(
        interner: &mut Interner,
        id: TypeParamId,
        name: &str,
        constraint: Option<TypeId>,
    ) -> TypeId {
        let param_ty = interner.intern_type_param(id, name);
        interner.intern_function(FunctionType {
            type_params: vec![GenericTypeParam {
                id,
                constraint,
                default: None,
            }],
            receiver: None,
            params: vec![ParameterType::required("value", param_ty)],
            ret: param_ty,
        })
    }

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let source_id = TypeParamId(10_001);
    let target_id = TypeParamId(10_002);
    let source_param_ty = interner.intern_type_param(source_id, "T");
    let target_param_ty = interner.intern_type_param(target_id, "U");
    let alpha_source = interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id: source_id,
            constraint: None,
            default: None,
        }],
        receiver: None,
        params: vec![ParameterType::required("value", source_param_ty)],
        ret: source_param_ty,
    });
    let alpha_target = interner.intern_function(FunctionType {
        type_params: vec![GenericTypeParam {
            id: target_id,
            constraint: None,
            default: None,
        }],
        receiver: None,
        params: vec![ParameterType::required("value", target_param_ty)],
        ret: target_param_ty,
    });
    let number_bound = generic_identity(&mut interner, TypeParamId(10_003), "T", Some(wk.number));
    let string_bound = generic_identity(&mut interner, TypeParamId(10_004), "U", Some(wk.string));
    let specific_string = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required("value", wk.string)],
        ret: wk.string,
    });

    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    assert!(rel.is_assignable(alpha_source, alpha_target).is_yes());
    assert!(rel.is_assignable(alpha_target, alpha_source).is_yes());
    assert!(rel.is_assignable(alpha_source, specific_string).is_yes());
    assert!(!rel.is_assignable(specific_string, alpha_source).is_yes());
    assert!(!rel.is_assignable(number_bound, string_bound).is_yes());
    assert!(!rel.is_assignable(string_bound, number_bound).is_yes());

    assert_eq!(
        rel.cache.get(RelationKey::new(
            source_param_ty,
            target_param_ty,
            RelationKind::Assignable,
        )),
        None,
        "alpha-aligned binder children must bypass the durable raw-id cache"
    );
}

/// B41 — construct signatures may specialize an unconstrained extra source
/// binder from a return-only occurrence, while a constrained return-only binder
/// continues to use its apparent type.
#[test]
fn construct_signature_extra_source_binders_follow_return_occurrence_rules() {
    use crate::types::repr::{FunctionType, GenericTypeParam, ParameterType, TypeParamId};

    fn binder(id: TypeParamId, constraint: Option<TypeId>) -> GenericTypeParam {
        GenericTypeParam {
            id,
            constraint,
            default: None,
        }
    }

    fn constructable(
        interner: &mut Interner,
        type_params: Vec<GenericTypeParam>,
        parameter: TypeId,
        ret: TypeId,
    ) -> TypeId {
        let signature = interner.intern_function(FunctionType {
            type_params,
            receiver: None,
            params: vec![ParameterType::required("value", parameter)],
            ret,
        });
        interner.intern_object(ObjectType {
            construct_signatures: vec![signature],
            ..Default::default()
        })
    }

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    let source_shared = interner.intern_type_param(TypeParamId(10_051), "T");
    let source_extra = interner.intern_type_param(TypeParamId(10_052), "U");
    let target_shared = interner.intern_type_param(TypeParamId(10_053), "T");
    let source_extra_return = constructable(
        &mut interner,
        vec![
            binder(TypeParamId(10_051), None),
            binder(TypeParamId(10_052), None),
        ],
        source_shared,
        source_extra,
    );
    let target_one_return = constructable(
        &mut interner,
        vec![binder(TypeParamId(10_053), None)],
        target_shared,
        target_shared,
    );

    let parameter_extra = interner.intern_type_param(TypeParamId(10_055), "U");
    let parameter_target = interner.intern_type_param(TypeParamId(10_056), "T");
    let source_extra_parameter = constructable(
        &mut interner,
        vec![
            binder(TypeParamId(10_054), None),
            binder(TypeParamId(10_055), None),
        ],
        parameter_extra,
        parameter_extra,
    );
    let target_one_parameter = constructable(
        &mut interner,
        vec![binder(TypeParamId(10_056), None)],
        parameter_target,
        parameter_target,
    );

    let base = interner.intern_object(ObjectType {
        properties: vec![prop("base", wk.string)],
        ..Default::default()
    });
    let derived = interner.intern_object(ObjectType {
        properties: vec![prop("base", wk.string), prop("derived", wk.string)],
        ..Default::default()
    });
    let constrained_shared = interner.intern_type_param(TypeParamId(10_057), "T");
    let constrained_extra = interner.intern_type_param(TypeParamId(10_058), "U");
    let constrained_target = interner.intern_type_param(TypeParamId(10_059), "T");
    let source_extra_constrained = constructable(
        &mut interner,
        vec![
            binder(TypeParamId(10_057), None),
            binder(TypeParamId(10_058), Some(derived)),
        ],
        constrained_shared,
        constrained_extra,
    );
    let target_base_return = constructable(
        &mut interner,
        vec![binder(TypeParamId(10_059), None)],
        constrained_target,
        base,
    );

    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    assert!(
        rel.is_assignable(source_extra_return, target_one_return)
            .is_yes(),
        "an unconstrained return-only source binder specializes to the target return"
    );
    assert!(
        !rel.is_assignable(target_one_return, source_extra_return)
            .is_yes(),
        "the extra target binder remains universal"
    );
    assert!(
        rel.is_assignable(source_extra_parameter, target_one_parameter)
            .is_yes(),
        "a parameter-occurring source binder retains specialization"
    );
    assert!(
        !rel.is_assignable(target_one_parameter, source_extra_parameter)
            .is_yes(),
        "a parameter-occurring extra target binder remains universal"
    );
    assert!(
        rel.is_assignable(source_extra_constrained, target_base_return)
            .is_yes(),
        "a constrained return-only source binder uses its apparent type"
    );
    assert!(
        !rel.is_assignable(target_base_return, source_extra_constrained)
            .is_yes(),
        "a constrained extra target binder remains universal"
    );
}

/// B41 — recursive construct-signature relation must retain the occurrence-aware
/// extra-binder meaning independently of a preceding reverse query.
#[test]
fn recursive_construct_signature_extra_source_return_binder_is_order_independent() {
    use crate::types::repr::{FunctionType, GenericTypeParam, ParameterType, TypeParamId};

    fn build(interner: &mut Interner) -> (TypeId, TypeId) {
        let source = interner.reserve_object();
        let target = interner.reserve_object();
        let source_shared = interner.intern_type_param(TypeParamId(10_061), "T");
        let source_extra = interner.intern_type_param(TypeParamId(10_062), "U");
        let target_shared = interner.intern_type_param(TypeParamId(10_063), "T");
        let source_signature = interner.intern_function(FunctionType {
            type_params: vec![
                GenericTypeParam {
                    id: TypeParamId(10_061),
                    constraint: None,
                    default: None,
                },
                GenericTypeParam {
                    id: TypeParamId(10_062),
                    constraint: None,
                    default: None,
                },
            ],
            receiver: None,
            params: vec![ParameterType::required("value", source_shared)],
            ret: source_extra,
        });
        let target_signature = interner.intern_function(FunctionType {
            type_params: vec![GenericTypeParam {
                id: TypeParamId(10_063),
                constraint: None,
                default: None,
            }],
            receiver: None,
            params: vec![ParameterType::required("value", target_shared)],
            ret: target_shared,
        });
        interner.fill_object(
            source,
            ObjectType {
                properties: vec![prop("next", source)],
                construct_signatures: vec![source_signature],
                ..Default::default()
            },
        );
        interner.fill_object(
            target,
            ObjectType {
                properties: vec![prop("next", target)],
                construct_signatures: vec![target_signature],
                ..Default::default()
            },
        );
        (source, target)
    }

    let first_order = {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let (source, target) = build(&mut interner);
        let store = interner.store();
        let mut rel = Relater::new(store, wk);
        assert!(!rel.is_assignable(target, source).is_yes());
        rel.is_assignable(source, target).is_yes()
    };
    let second_order = {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let (source, target) = build(&mut interner);
        let store = interner.store();
        let mut rel = Relater::new(store, wk);
        rel.is_assignable(source, target).is_yes()
    };

    assert!(first_order);
    assert_eq!(first_order, second_order);
}

/// B70 — recursive generic receivers retain the existing cycle rule while the
/// binder-local child verdicts stay isolated from cache/query ordering.
///
/// The generic binder occurs only in the receiver. Positional parameters and
/// returns are fixed, so the failing verdict below can only come from receiver
/// comparison.
#[test]
fn recursive_generic_signature_relations_are_order_independent() {
    use crate::types::repr::{FunctionType, GenericTypeParam, ParameterType, TypeParamId};

    fn generic_receiver(
        interner: &mut Interner,
        id: TypeParamId,
        recursive_self: TypeId,
        tag: TypeId,
    ) -> TypeId {
        let param_ty = interner.intern_type_param(id, "T");
        let receiver = interner.intern_object(ObjectType {
            properties: vec![
                prop("next", recursive_self),
                prop("tag", tag),
                prop("value", param_ty),
            ],
            ..Default::default()
        });
        interner.intern_function(FunctionType {
            type_params: vec![GenericTypeParam {
                id,
                constraint: None,
                default: None,
            }],
            receiver: Some(receiver),
            params: vec![ParameterType::required(
                "value",
                interner.well_known().number,
            )],
            ret: interner.well_known().string,
        })
    }

    fn build(interner: &mut Interner) -> (TypeId, TypeId, TypeId, TypeId) {
        let wk = interner.well_known();
        let (left, right, number, string) = (
            interner.reserve_object(),
            interner.reserve_object(),
            interner.reserve_object(),
            interner.reserve_object(),
        );
        let left_map = generic_receiver(interner, TypeParamId(10_101), left, wk.unknown);
        let right_map = generic_receiver(interner, TypeParamId(10_102), right, wk.unknown);
        let number_map = generic_receiver(interner, TypeParamId(10_103), number, wk.number);
        let string_map = generic_receiver(interner, TypeParamId(10_104), string, wk.string);
        for (object, map) in [
            (left, left_map),
            (right, right_map),
            (number, number_map),
            (string, string_map),
        ] {
            interner.fill_object(
                object,
                ObjectType {
                    properties: vec![prop("map", map), prop("next", object)],
                    ..Default::default()
                },
            );
        }
        (left, right, number, string)
    }

    let first_order = {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let (left, right, number, string) = build(&mut interner);
        let store = interner.store();
        let mut rel = Relater::new(store, wk);
        assert!(!rel.is_assignable(string, number).is_yes());
        assert!(rel.is_assignable(left, right).is_yes());
        assert!(rel.is_assignable(right, left).is_yes());
        rel.is_assignable(number, string).is_yes()
    };

    let second_order = {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let (_left, _right, number, string) = build(&mut interner);
        let store = interner.store();
        let mut rel = Relater::new(store, wk);
        rel.is_assignable(number, string).is_yes()
    };

    assert_eq!(first_order, second_order);
    assert!(!second_order);
}

#[derive(Copy, Clone)]
enum RecursiveGenericShape {
    Direct,
    StructuralPayload,
    NestedGenericCallback,
}

fn recursive_generic_pair(
    interner: &mut Interner,
    shape: RecursiveGenericShape,
    left_outer_id: TypeParamId,
    right_outer_id: TypeParamId,
    left_outer_constraint: Option<TypeId>,
    right_outer_constraint: Option<TypeId>,
) -> (TypeId, TypeId) {
    use crate::types::repr::{FunctionType, GenericTypeParam, ParameterType};

    fn generic_param(id: TypeParamId, constraint: Option<TypeId>) -> GenericTypeParam {
        GenericTypeParam {
            id,
            constraint,
            default: None,
        }
    }

    fn payload(interner: &mut Interner, self_ty: TypeId, item: TypeId) -> TypeId {
        interner.intern_object(ObjectType {
            properties: vec![prop("self", self_ty), prop("item", item)],
            ..Default::default()
        })
    }

    let left = interner.reserve_object();
    let right = interner.reserve_object();
    let left_outer = interner.intern_type_param(left_outer_id, "T");
    let right_outer = interner.intern_type_param(right_outer_id, "U");

    let (left_map, right_map) = match shape {
        RecursiveGenericShape::Direct => {
            let left_map = interner.intern_function(FunctionType {
                type_params: vec![generic_param(left_outer_id, left_outer_constraint)],
                receiver: None,
                params: vec![ParameterType::required("value", left)],
                ret: left,
            });
            let right_map = interner.intern_function(FunctionType {
                type_params: vec![generic_param(right_outer_id, right_outer_constraint)],
                receiver: None,
                params: vec![ParameterType::required("value", right)],
                ret: right,
            });
            (left_map, right_map)
        }
        RecursiveGenericShape::StructuralPayload => {
            let left_payload = payload(interner, left, left_outer);
            let right_payload = payload(interner, right, right_outer);
            let left_map = interner.intern_function(FunctionType {
                type_params: vec![generic_param(left_outer_id, left_outer_constraint)],
                receiver: None,
                params: vec![ParameterType::required("value", left_payload)],
                ret: left_payload,
            });
            let right_map = interner.intern_function(FunctionType {
                type_params: vec![generic_param(right_outer_id, right_outer_constraint)],
                receiver: None,
                params: vec![ParameterType::required("value", right_payload)],
                ret: right_payload,
            });
            (left_map, right_map)
        }
        RecursiveGenericShape::NestedGenericCallback => {
            let left_callback_id = TypeParamId(left_outer_id.0 + 1);
            let right_callback_id = TypeParamId(right_outer_id.0 + 1);
            let left_callback = interner.intern_type_param(left_callback_id, "V");
            let right_callback = interner.intern_type_param(right_callback_id, "W");
            let left_input = payload(interner, left, left_outer);
            let right_input = payload(interner, right, right_outer);
            let left_output = payload(interner, left, left_callback);
            let right_output = payload(interner, right, right_callback);
            let left_callback = interner.intern_function(FunctionType {
                type_params: vec![generic_param(left_callback_id, None)],
                receiver: None,
                params: vec![ParameterType::required("value", left_input)],
                ret: left_output,
            });
            let right_callback = interner.intern_function(FunctionType {
                type_params: vec![generic_param(right_callback_id, None)],
                receiver: None,
                params: vec![ParameterType::required("value", right_input)],
                ret: right_output,
            });
            let left_map = interner.intern_function(FunctionType {
                type_params: vec![generic_param(left_outer_id, left_outer_constraint)],
                receiver: None,
                params: vec![ParameterType::required("callback", left_callback)],
                ret: left_input,
            });
            let right_map = interner.intern_function(FunctionType {
                type_params: vec![generic_param(right_outer_id, right_outer_constraint)],
                receiver: None,
                params: vec![ParameterType::required("callback", right_callback)],
                ret: right_input,
            });
            (left_map, right_map)
        }
    };

    for (object, map) in [(left, left_map), (right, right_map)] {
        interner.fill_object(
            object,
            ObjectType {
                properties: vec![prop("self", object), prop("map", map)],
                ..Default::default()
            },
        );
    }
    (left, right)
}

/// A direct `Left.map<T>(value: Left): Left` relation must reach its recursive
/// generic frame again and terminate under the same effective binder environment.
#[test]
fn recursive_direct_generic_methods_terminate_in_both_directions() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let (left, right) = recursive_generic_pair(
        &mut interner,
        RecursiveGenericShape::Direct,
        TypeParamId(10_301),
        TypeParamId(10_302),
        None,
        None,
    );
    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    assert!(rel.is_assignable(left, right).is_yes());
    assert!(rel.is_assignable(right, left).is_yes());
}

/// A recursively structural `{ self; item: T }` payload keeps its alpha-aligned
/// item binder as the relation descends through function parameters and returns.
#[test]
fn recursive_structural_generic_payload_terminates_in_both_directions() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let (left, right) = recursive_generic_pair(
        &mut interner,
        RecursiveGenericShape::StructuralPayload,
        TypeParamId(10_311),
        TypeParamId(10_312),
        None,
        None,
    );
    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    assert!(rel.is_assignable(left, right).is_yes());
    assert!(rel.is_assignable(right, left).is_yes());
}

/// An outer method binder plus a generic callback binder must also collapse to
/// the same recursive environment, rather than growing one unique stack frame per descent.
#[test]
fn recursive_nested_generic_callback_terminates_in_both_directions() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let (left, right) = recursive_generic_pair(
        &mut interner,
        RecursiveGenericShape::NestedGenericCallback,
        TypeParamId(10_321),
        TypeParamId(10_322),
        None,
        None,
    );
    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    assert!(rel.is_assignable(left, right).is_yes());
    assert!(rel.is_assignable(right, left).is_yes());
}

/// Distinct contextual specializations of an otherwise recursive structural
/// method must remain distinct in either query order.
#[test]
fn recursive_structural_specializations_reject_in_both_orders() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let (number, string) = recursive_generic_pair(
        &mut interner,
        RecursiveGenericShape::StructuralPayload,
        TypeParamId(10_331),
        TypeParamId(10_332),
        Some(wk.number),
        Some(wk.string),
    );
    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    assert!(!rel.is_assignable(string, number).is_yes());
    assert!(!rel.is_assignable(number, string).is_yes());
}

/// Equivalent nested frames have identical effective meaning and must share an
/// in-flight cycle identity; a reversed alpha alignment must not share it.
#[test]
fn cycle_stack_keys_are_semantic_not_frame_allocations() {
    use crate::types::repr::GenericTypeParam;

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let source_id = TypeParamId(10_341);
    let target_id = TypeParamId(10_342);
    let source_ty = interner.intern_type_param(source_id, "T");
    let target_ty = interner.intern_type_param(target_id, "U");
    let source = interner.intern_object(ObjectType {
        properties: vec![prop("value", source_ty)],
        ..Default::default()
    });
    let target = interner.intern_object(ObjectType {
        properties: vec![prop("value", target_ty)],
        ..Default::default()
    });
    let source_binders = vec![GenericTypeParam {
        id: source_id,
        constraint: None,
        default: None,
    }];
    let target_binders = vec![GenericTypeParam {
        id: target_id,
        constraint: None,
        default: None,
    }];
    let raw = RelationKey::new(source, target, RelationKind::Assignable);

    let store = interner.store();
    let mut rel = Relater::new(store, wk);
    let single = rel.with_binder_context(
        BinderRelationContext::aligned(&source_binders, &target_binders),
        |relater| relater.stack_relation_key(raw),
    );
    let repeated = rel.with_binder_context(
        BinderRelationContext::aligned(&source_binders, &target_binders),
        |relater| {
            relater.with_binder_context(
                BinderRelationContext::aligned(&source_binders, &target_binders),
                |relater| relater.stack_relation_key(raw),
            )
        },
    );
    let reversed = rel.with_binder_context(
        BinderRelationContext::aligned(&target_binders, &source_binders),
        |relater| relater.stack_relation_key(raw),
    );

    assert!(single == repeated);
    assert!(single != reversed);
}

/// Alpha-aligned method binders give deferred indexed accesses the same local
/// meaning; the map operand still participates in the relation.
#[test]
fn aligned_deferred_indexed_accesses_relate_by_map_and_binder() {
    use crate::types::repr::{GenericTypeParam, TypeParamId};

    fn event_map(interner: &mut Interner, payload: TypeId) -> TypeId {
        let event = interner.intern_object(ObjectType {
            properties: vec![prop("payload", payload)],
            ..Default::default()
        });
        interner.intern_object(ObjectType {
            properties: vec![prop("change", event)],
            ..Default::default()
        })
    }

    fn binder(id: TypeParamId, constraint: TypeId) -> GenericTypeParam {
        GenericTypeParam {
            id,
            constraint: Some(constraint),
            default: None,
        }
    }

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let same_map = event_map(&mut interner, wk.number);
    let incompatible_map = event_map(&mut interner, wk.string);
    let source_id = TypeParamId(10_351);
    let target_id = TypeParamId(10_352);
    let source_key = interner.intern_type_param(source_id, "K");
    let target_key = interner.intern_type_param(target_id, "K");
    let source_access = interner.intern_deferred_indexed_access(same_map, source_key);
    let target_access = interner.intern_deferred_indexed_access(same_map, target_key);
    let incompatible_access = interner.intern_deferred_indexed_access(incompatible_map, target_key);
    let source_binders = vec![binder(source_id, interner.intern_keyof(same_map))];
    let target_binders = vec![binder(target_id, interner.intern_keyof(same_map))];
    let incompatible_binders = vec![binder(target_id, interner.intern_keyof(incompatible_map))];

    let store = interner.store();
    let mut rel = Relater::new(store, wk);
    let aligned = rel.with_binder_context(
        BinderRelationContext::aligned(&source_binders, &target_binders),
        |relater| relater.is_assignable(source_access, target_access).is_yes(),
    );
    let incompatible = rel.with_binder_context(
        BinderRelationContext::aligned(&source_binders, &incompatible_binders),
        |relater| {
            relater
                .is_assignable(source_access, incompatible_access)
                .is_yes()
        },
    );

    assert!(
        !incompatible,
        "different event payload maps remain distinct"
    );
    assert!(
        aligned,
        "alpha-aligned K parameters denote the same map lookup"
    );
}

/// An inner generic method binder shadows an outer reuse of the same persistent
/// source id; deferred indexed access must observe only that effective alignment.
#[test]
fn deferred_indexed_access_uses_innermost_binder_alignment() {
    use crate::types::repr::{GenericTypeParam, TypeParamId};

    fn binder(id: TypeParamId) -> GenericTypeParam {
        GenericTypeParam {
            id,
            constraint: None,
            default: None,
        }
    }

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let map = interner.intern_object(ObjectType {
        properties: vec![prop("change", wk.number)],
        ..Default::default()
    });
    let source_id = TypeParamId(10_371);
    let outer_target_id = TypeParamId(10_372);
    let inner_target_id = TypeParamId(10_373);
    let source_key = interner.intern_type_param(source_id, "S");
    let outer_target_key = interner.intern_type_param(outer_target_id, "T1");
    let inner_target_key = interner.intern_type_param(inner_target_id, "T2");
    let source_access = interner.intern_deferred_indexed_access(map, source_key);
    let outer_target_access = interner.intern_deferred_indexed_access(map, outer_target_key);
    let inner_target_access = interner.intern_deferred_indexed_access(map, inner_target_key);
    let source_binders = vec![binder(source_id)];
    let outer_target_binders = vec![binder(outer_target_id)];
    let inner_target_binders = vec![binder(inner_target_id)];

    let store = interner.store();
    let mut rel = Relater::new(store, wk);
    let (effective_inner, stale_outer) = rel.with_binder_context(
        BinderRelationContext::aligned(&source_binders, &outer_target_binders),
        |relater| {
            relater.with_binder_context(
                BinderRelationContext::aligned(&source_binders, &inner_target_binders),
                |relater| {
                    (
                        relater
                            .is_assignable(source_access, inner_target_access)
                            .is_yes(),
                        relater
                            .is_assignable(source_access, outer_target_access)
                            .is_yes(),
                    )
                },
            )
        },
    );

    assert!(effective_inner, "the inner S -> T2 alignment is effective");
    assert!(
        !stale_outer,
        "the shadowed outer S -> T1 alignment is not effective"
    );
}

/// B41/WU4 — an in-flight raw object pair under one binder environment must
/// not short-circuit the same pair under a nested environment that specializes
/// its parameter differently. The nested result must match a standalone query.
#[test]
fn cycle_stack_distinguishes_nested_binder_contexts() {
    use crate::types::repr::{GenericTypeParam, TypeParamId};

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let param_id = TypeParamId(10_201);
    let param_ty = interner.intern_type_param(param_id, "T");
    let source = interner.intern_object(ObjectType {
        properties: vec![prop("value", param_ty)],
        ..Default::default()
    });
    let target = interner.intern_object(ObjectType {
        properties: vec![prop("value", wk.string)],
        ..Default::default()
    });
    let binders = vec![GenericTypeParam {
        id: param_id,
        constraint: None,
        default: None,
    }];

    let store = interner.store();
    let mut rel = Relater::new(store, wk);
    let standalone = {
        let mut number_specialization = BinderRelationContext::source_specialization(&binders);
        number_specialization
            .source_instantiations
            .insert(param_id, wk.number);
        rel.with_binder_context(number_specialization, |relater| {
            relater.is_assignable(source, target).is_yes()
        })
    };
    let nested = {
        let outer = BinderRelationContext::source_specialization(&binders);
        rel.with_binder_context(outer, |relater| {
            let raw = RelationKey::new(source, target, RelationKind::Assignable);
            let outer_stack_key = relater.stack_relation_key(raw);
            relater.stack.insert(outer_stack_key.clone());

            let mut number_specialization = BinderRelationContext::source_specialization(&binders);
            number_specialization
                .source_instantiations
                .insert(param_id, wk.number);
            let nested = relater.with_binder_context(number_specialization, |relater| {
                let nested_stack_key = relater.stack_relation_key(raw);
                assert!(!relater.stack.contains(&nested_stack_key));
                relater.is_assignable(source, target).is_yes()
            });

            relater.stack.remove(&outer_stack_key);
            nested
        })
    };

    assert_eq!(nested, standalone);
    assert!(!standalone);
}

/// M32 function-shape relation: optional/default slots lower required arity, rest
/// parameters compare by their element slots, and an optional target contributes an
/// explicit `undefined` possibility in the contravariant parameter direction.
#[test]
fn function_optional_and_rest_shape_assignability() {
    use crate::types::repr::{FunctionType, ParameterType};

    fn req(name: &str, ty: TypeId) -> ParameterType {
        ParameterType::required(name, ty)
    }

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let string_arr = interner.intern_array(wk.string);
    let number_arr = interner.intern_array(wk.number);

    let required = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![req("a", wk.number), req("b", wk.string)],
        ret: wk.void,
    });
    let optional = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![req("a", wk.number), ParameterType::optional("b", wk.string)],
        ret: wk.void,
    });
    let rest = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![req("a", wk.number), ParameterType::rest("b", string_arr)],
        ret: wk.void,
    });
    let number_rest = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![req("a", wk.number), ParameterType::rest("b", number_arr)],
        ret: wk.void,
    });

    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    assert!(rel.is_assignable(optional, required).is_yes());
    assert!(rel.is_assignable(optional, rest).is_yes());

    assert!(!rel.is_assignable(required, optional).is_yes());
    assert!(!rel.is_assignable(rest, optional).is_yes());
    assert!(!rel.is_assignable(rest, number_rest).is_yes());
}

/// WU7 callable rest/fixed parity: a target rest absorbs compatible surplus
/// fixed source slots, while a source rest covers every remaining target slot.
#[test]
fn function_rest_and_fixed_slot_assignability() {
    use crate::types::repr::{FunctionType, ParameterType, TupleRestType, TupleType};

    fn function(params: Vec<ParameterType>, ret: TypeId) -> FunctionType {
        FunctionType {
            type_params: Vec::new(),
            receiver: None,
            params,
            ret,
        }
    }

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let number_arr = interner.intern_array(wk.number);
    let string_arr = interner.intern_array(wk.string);
    let unknown_arr = interner.intern_array(wk.unknown);
    let never_arr = interner.intern_array(wk.never);
    let rest_tuple = interner.intern_tuple_type(TupleType::with_rest(
        vec![wk.number, wk.string],
        TupleRestType::new(1, number_arr),
    ));
    let string_suffix_tuple = interner.intern_tuple_type(TupleType::with_rest(
        vec![wk.string],
        TupleRestType::new(0, number_arr),
    ));
    let number_suffix_tuple = interner.intern_tuple_type(TupleType::with_rest(
        vec![wk.number],
        TupleRestType::new(0, number_arr),
    ));
    let unknown_string_suffix_tuple = interner.intern_tuple_type(TupleType::with_rest(
        vec![wk.string],
        TupleRestType::new(0, unknown_arr),
    ));
    let never_string_suffix_tuple = interner.intern_tuple_type(TupleType::with_rest(
        vec![wk.string],
        TupleRestType::new(0, never_arr),
    ));
    let unknown_suffix_tuple = interner.intern_tuple_type(TupleType::with_rest(
        vec![wk.unknown],
        TupleRestType::new(0, unknown_arr),
    ));
    let unknown_prefix_suffix_tuple = interner.intern_tuple_type(TupleType::with_rest(
        vec![wk.unknown, wk.unknown],
        TupleRestType::new(1, unknown_arr),
    ));

    let target_rest = interner.intern_function(function(
        vec![ParameterType::rest("args", number_arr)],
        wk.number,
    ));
    let source_one_fixed = interner.intern_function(function(
        vec![ParameterType::required("x", wk.number)],
        wk.number,
    ));
    let source_bad_fixed = interner.intern_function(function(
        vec![ParameterType::required("x", wk.string)],
        wk.number,
    ));
    let target_fixed_rest = interner.intern_function(function(
        vec![
            ParameterType::required("x", wk.number),
            ParameterType::rest("rest", number_arr),
        ],
        wk.number,
    ));
    let source_two_fixed = interner.intern_function(function(
        vec![
            ParameterType::required("x", wk.number),
            ParameterType::required("y", wk.number),
        ],
        wk.number,
    ));
    let source_bad_second_fixed = interner.intern_function(function(
        vec![
            ParameterType::required("x", wk.number),
            ParameterType::required("y", wk.string),
        ],
        wk.number,
    ));
    let target_optional_rest = interner.intern_function(function(
        vec![
            ParameterType::required("x", wk.number),
            ParameterType::optional("y", wk.string),
            ParameterType::rest("rest", number_arr),
        ],
        wk.number,
    ));
    let source_fixed_number_rest = interner.intern_function(function(
        vec![
            ParameterType::required("x", wk.number),
            ParameterType::rest("rest", number_arr),
        ],
        wk.number,
    ));
    let target_optional_prefix_rest = interner.intern_function(function(
        vec![
            ParameterType::optional("x", wk.number),
            ParameterType::optional("y", wk.string),
            ParameterType::rest("rest", number_arr),
        ],
        wk.number,
    ));
    let source_number_rest = interner.intern_function(function(
        vec![ParameterType::rest("args", number_arr)],
        wk.number,
    ));
    let target_string_rest = interner.intern_function(function(
        vec![
            ParameterType::required("x", wk.number),
            ParameterType::rest("rest", string_arr),
        ],
        wk.number,
    ));
    let target_rest_suffix = interner.intern_function(function(
        vec![ParameterType::rest("args", rest_tuple)],
        wk.number,
    ));
    let source_string_suffix = interner.intern_function(function(
        vec![ParameterType::rest("source", string_suffix_tuple)],
        wk.number,
    ));
    let source_number_suffix = interner.intern_function(function(
        vec![ParameterType::rest("source", number_suffix_tuple)],
        wk.number,
    ));
    let target_fixed_pair = interner.intern_function(function(
        vec![
            ParameterType::required("first", wk.number),
            ParameterType::required("second", wk.number),
        ],
        wk.number,
    ));
    let target_moving_suffix = interner.intern_function(function(
        vec![ParameterType::rest("target", string_suffix_tuple)],
        wk.number,
    ));
    let source_required_unknown_prefix_rest = interner.intern_function(function(
        vec![
            ParameterType::required("first", wk.unknown),
            ParameterType::rest("rest", unknown_arr),
        ],
        wk.number,
    ));
    let source_optional_unknown_prefix_rest = interner.intern_function(function(
        vec![
            ParameterType::optional("first", wk.unknown),
            ParameterType::rest("rest", unknown_arr),
        ],
        wk.number,
    ));
    let source_defaulted_unknown_prefix_rest = interner.intern_function(function(
        vec![
            ParameterType::defaulted("first", wk.unknown),
            ParameterType::rest("rest", unknown_arr),
        ],
        wk.number,
    ));
    let source_finite_optional = interner.intern_function(function(
        vec![ParameterType::optional("first", wk.unknown)],
        wk.number,
    ));
    let source_finite_defaulted = interner.intern_function(function(
        vec![ParameterType::defaulted("first", wk.unknown)],
        wk.number,
    ));
    let source_zero_finite = interner.intern_function(function(Vec::new(), wk.number));
    let target_never_moving_suffix = interner.intern_function(function(
        vec![ParameterType::rest("target", never_string_suffix_tuple)],
        wk.number,
    ));
    let target_pure_never_rest = interner.intern_function(function(
        vec![ParameterType::rest("target", never_arr)],
        wk.number,
    ));
    let source_optional_prefix_and_suffix = interner.intern_function(function(
        vec![
            ParameterType::optional("first", wk.unknown),
            ParameterType::rest("rest", unknown_string_suffix_tuple),
        ],
        wk.number,
    ));
    let target_single_string = interner.intern_function(function(
        vec![ParameterType::required("value", wk.string)],
        wk.number,
    ));
    let source_prefix_and_suffix = interner.intern_function(function(
        vec![
            ParameterType::required("first", wk.unknown),
            ParameterType::rest("rest", unknown_string_suffix_tuple),
        ],
        wk.number,
    ));
    let target_prefix_and_moving_suffix = interner.intern_function(function(
        vec![
            ParameterType::required("first", wk.number),
            ParameterType::rest("rest", string_suffix_tuple),
        ],
        wk.number,
    ));
    let target_rest_copy = interner.intern_function(function(
        vec![ParameterType::rest("target", number_arr)],
        wk.number,
    ));
    let source_one_unknown_fixed = interner.intern_function(function(
        vec![ParameterType::required("value", wk.unknown)],
        wk.number,
    ));
    let source_unknown_rest = interner.intern_function(function(
        vec![ParameterType::rest("source", unknown_arr)],
        wk.number,
    ));
    let source_unknown_suffix = interner.intern_function(function(
        vec![ParameterType::rest("source", unknown_suffix_tuple)],
        wk.number,
    ));
    let target_prefix_moving_unknown_suffix = interner.intern_function(function(
        vec![ParameterType::rest("target", unknown_prefix_suffix_tuple)],
        wk.number,
    ));
    let target_fixed_unknown_rest = interner.intern_function(function(
        vec![
            ParameterType::required("value", wk.unknown),
            ParameterType::rest("rest", unknown_arr),
        ],
        wk.number,
    ));
    let target_single_unknown = interner.intern_function(function(
        vec![ParameterType::required("value", wk.unknown)],
        wk.number,
    ));
    let target_optional_unknown = interner.intern_function(function(
        vec![ParameterType::optional("value", wk.unknown)],
        wk.number,
    ));

    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    assert!(rel.is_assignable(source_one_fixed, target_rest).is_yes());
    assert!(rel
        .is_assignable(source_two_fixed, target_fixed_rest)
        .is_yes());
    assert!(!rel.is_assignable(source_bad_fixed, target_rest).is_yes());
    assert!(!rel
        .is_assignable(source_bad_second_fixed, target_fixed_rest)
        .is_yes());
    assert!(!rel
        .is_assignable(source_fixed_number_rest, target_optional_rest)
        .is_yes());
    assert!(!rel
        .is_assignable(source_number_rest, target_optional_prefix_rest)
        .is_yes());
    assert!(!rel
        .is_assignable(source_fixed_number_rest, target_string_rest)
        .is_yes());
    assert!(!rel
        .is_assignable(source_number_rest, target_rest_suffix)
        .is_yes());
    assert!(!rel
        .is_assignable(source_string_suffix, target_rest)
        .is_yes());
    assert!(rel.is_assignable(target_rest_copy, target_rest).is_yes());
    assert!(!rel
        .is_assignable(source_string_suffix, target_fixed_pair)
        .is_yes());
    assert!(rel
        .is_assignable(source_number_suffix, target_fixed_pair)
        .is_yes());
    assert!(!rel
        .is_assignable(source_required_unknown_prefix_rest, target_moving_suffix)
        .is_yes());
    assert!(rel
        .is_assignable(source_optional_unknown_prefix_rest, target_moving_suffix)
        .is_yes());
    assert!(rel
        .is_assignable(source_defaulted_unknown_prefix_rest, target_moving_suffix)
        .is_yes());
    assert!(!rel
        .is_assignable(source_finite_optional, target_moving_suffix)
        .is_yes());
    assert!(!rel
        .is_assignable(source_finite_defaulted, target_moving_suffix)
        .is_yes());
    assert!(rel
        .is_assignable(source_zero_finite, target_moving_suffix)
        .is_yes());
    assert!(!rel
        .is_assignable(source_bad_fixed, target_never_moving_suffix)
        .is_yes());
    assert!(rel
        .is_assignable(source_bad_fixed, target_pure_never_rest)
        .is_yes());
    assert!(!rel
        .is_assignable(source_optional_prefix_and_suffix, target_single_string)
        .is_yes());
    assert!(rel
        .is_assignable(source_string_suffix, target_moving_suffix)
        .is_yes());
    assert!(!rel
        .is_assignable(source_prefix_and_suffix, target_moving_suffix)
        .is_yes());
    assert!(rel
        .is_assignable(source_prefix_and_suffix, target_prefix_and_moving_suffix)
        .is_yes());

    for (source, target, expected_index) in [
        (source_bad_second_fixed, target_fixed_rest, 1),
        (
            source_one_unknown_fixed,
            target_prefix_moving_unknown_suffix,
            0,
        ),
    ] {
        for _ in 0..2 {
            let Relation::No(chain) = rel.is_assignable(source, target) else {
                panic!("parameter mismatch must remain rejected");
            };
            assert!(matches!(
                chain.head(),
                Reason::Parameter { index, .. } if *index == expected_index
            ));
        }
    }

    // Representative rows from the 26-shape adversarial cross-product. The
    // fourth column is strict tsc 6.0.3; the fifth is typokat's intended WU7
    // contract. Only the two backlog-63 rows deliberately differ.
    let matrix = [
        (
            "finite source consumed by target prefix+moving suffix",
            source_one_unknown_fixed,
            target_prefix_moving_unknown_suffix,
            false,
            false,
        ),
        (
            "zero source ignores target prefix+moving suffix",
            source_zero_finite,
            target_prefix_moving_unknown_suffix,
            true,
            true,
        ),
        (
            "pure source rest accepts target prefix+moving suffix",
            source_unknown_rest,
            target_prefix_moving_unknown_suffix,
            true,
            true,
        ),
        (
            "optional source prefix+rest accepts target prefix+moving suffix",
            source_optional_unknown_prefix_rest,
            target_prefix_moving_unknown_suffix,
            true,
            true,
        ),
        (
            "source moving suffix rejects target fixed prefix+pure rest",
            source_unknown_suffix,
            target_fixed_unknown_rest,
            false,
            false,
        ),
        (
            "pure source rest accepts target fixed prefix+pure rest",
            source_unknown_rest,
            target_fixed_unknown_rest,
            true,
            true,
        ),
        (
            "source moving suffix accepts one fixed target",
            source_unknown_suffix,
            target_single_unknown,
            true,
            true,
        ),
        (
            "pure never target rest accepts fixed source",
            source_bad_fixed,
            target_pure_never_rest,
            true,
            true,
        ),
        (
            "backlog 63: moving suffix source against zero target",
            source_unknown_suffix,
            source_zero_finite,
            true,
            false,
        ),
        (
            "backlog 63: required prefix+rest source against optional target",
            source_required_unknown_prefix_rest,
            target_optional_unknown,
            true,
            false,
        ),
    ];
    for (case, source, target, tsc_assignable, expected_assignable) in matrix {
        let actual = rel.is_assignable(source, target).is_yes();
        assert_eq!(
            actual, expected_assignable,
            "{case}: strict tsc assignable={tsc_assignable}"
        );
    }
}

/// Callable parameter traversal must stay linear in a long optional tail.
#[test]
fn function_optional_tail_relation_work_is_linear() {
    const OPTIONALS: usize = 256;

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let calibration_source = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required("source", wk.unknown)],
        ret: wk.void,
    });
    let calibration_target = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required("target", wk.unknown)],
        ret: wk.void,
    });
    let source = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: Vec::new(),
        ret: wk.void,
    });
    let target = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: (0..OPTIONALS)
            .map(|index| ParameterType::optional(format!("p{index}"), wk.unknown))
            .collect(),
        ret: wk.void,
    });

    reset_relation_measure();
    assert!(Relater::new(interner.store(), wk)
        .is_assignable(calibration_source, calibration_target)
        .is_yes());
    assert!(relation_measure().function_parameter_positions > 0);

    reset_relation_measure();
    assert!(Relater::new(interner.store(), wk)
        .is_assignable(source, target)
        .is_yes());
    let positions = relation_measure().function_parameter_positions;
    assert!(
        positions <= (OPTIONALS * 8) as u64,
        "optional-tail relation visited {positions} parameter positions for {OPTIONALS} slots"
    );
}

/// M17 array assignability is covariant in the element and recurses through
/// nested arrays; arrays and non-arrays do not relate.
#[test]
fn array_covariant_assignability() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    let num_arr = interner.intern_array(wk.number);
    let str_arr = interner.intern_array(wk.string);
    let never_arr = interner.intern_array(wk.never);
    let unknown_arr = interner.intern_array(wk.unknown);
    // Nested: number[][] and string[][].
    let num_arr_arr = interner.intern_array(num_arr);
    let str_arr_arr = interner.intern_array(str_arr);

    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    // Identity.
    assert!(
        rel.is_assignable(num_arr, num_arr).is_yes(),
        "number[] <: number[]"
    );

    // Covariant element: never[] <: number[] (never <: number); number[] NOT <:
    // never[] (number is not <: never).
    assert!(
        rel.is_assignable(never_arr, num_arr).is_yes(),
        "never[] <: number[]"
    );
    assert!(
        !rel.is_assignable(num_arr, never_arr).is_yes(),
        "number[] is NOT assignable to never[]"
    );

    // Covariant: number[] <: unknown[] (number <: unknown), but not the reverse.
    assert!(
        rel.is_assignable(num_arr, unknown_arr).is_yes(),
        "number[] <: unknown[]"
    );
    assert!(
        !rel.is_assignable(unknown_arr, num_arr).is_yes(),
        "unknown[] is NOT assignable to number[]"
    );

    // string[] is NOT assignable to number[] — the element fails as a leaf,
    // wrapped in an `ArrayElement` reason.
    match rel.is_assignable(str_arr, num_arr) {
        Relation::No(chain) => match chain.head() {
            Reason::ArrayElement { because, .. } => {
                assert!(matches!(**because, Reason::Leaf { .. }));
            }
            other => panic!("expected an ArrayElement reason, got {other:?}"),
        },
        Relation::Yes => panic!("string[] must NOT be assignable to number[]"),
    }

    // Nested recurses: number[][] <: number[][]; string[][] NOT <: number[][].
    assert!(
        rel.is_assignable(num_arr_arr, num_arr_arr).is_yes(),
        "number[][] <: number[][]"
    );
    assert!(
        !rel.is_assignable(str_arr_arr, num_arr_arr).is_yes(),
        "string[][] is NOT assignable to number[][]"
    );

    // An array is not assignable to a non-array, nor a non-array to an array.
    assert!(
        !rel.is_assignable(num_arr, wk.number).is_yes(),
        "number[] is NOT assignable to number"
    );
    assert!(
        !rel.is_assignable(wk.number, num_arr).is_yes(),
        "number is NOT assignable to number[]"
    );
}

#[test]
fn recursive_promise_like_generic_callbacks_preserve_outer_variance() {
    use crate::types::repr::{GenericTypeParam, TypeParamId};

    fn promise_like(interner: &mut Interner, value: TypeId, binder: TypeParamId) -> TypeId {
        let wk = interner.well_known();
        let promise = interner.reserve_object();
        let result = interner.intern_type_param(binder, "Result");
        let callback = interner.intern_function(FunctionType {
            type_params: Vec::new(),
            receiver: None,
            params: vec![ParameterType::required("value", value)],
            ret: result,
        });
        let onfulfilled = interner.union(vec![callback, wk.null, wk.undefined]);
        let then = interner.intern_function(FunctionType {
            type_params: vec![GenericTypeParam {
                id: binder,
                constraint: None,
                default: Some(value),
            }],
            receiver: None,
            params: vec![ParameterType::optional("onfulfilled", onfulfilled)],
            ret: promise,
        });
        interner.fill_object(
            promise,
            ObjectType {
                properties: vec![prop("then", then)],
                ..Default::default()
            },
        );
        promise
    }

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let numbers = promise_like(&mut interner, wk.number, TypeParamId(900));
    let strings = promise_like(&mut interner, wk.string, TypeParamId(901));
    let relation = Relater::new(interner.store(), wk).is_assignable(numbers, strings);
    assert!(
        matches!(relation, Relation::No(_)),
        "PromiseLike<number> must not be assignable to PromiseLike<string>: {relation:?}",
    );
}

#[test]
fn object_keyword_and_empty_structural_object_remain_distinct() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let empty = interner.intern_object(ObjectType::default());
    let shaped = interner.intern_object(ObjectType {
        properties: vec![prop("value", wk.number)],
        ..Default::default()
    });
    let function = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: Vec::new(),
        ret: wk.void,
    });
    let array = interner.intern_array(wk.number);
    let thenable = interner.intern_object(ObjectType {
        properties: vec![prop("then", function)],
        ..Default::default()
    });
    let object_thenable = interner.intersection(vec![wk.object, thenable]);

    for source in [shaped, function, array] {
        assert!(
            Relater::new(interner.store(), wk)
                .is_assignable(source, wk.object)
                .is_yes(),
            "non-primitive {source:?} must satisfy object",
        );
        assert!(
            Relater::new(interner.store(), wk)
                .is_assignable(source, empty)
                .is_yes(),
            "non-primitive {source:?} must satisfy {{}}",
        );
    }
    for source in [wk.number, wk.string, wk.boolean] {
        assert!(
            matches!(
                Relater::new(interner.store(), wk).is_assignable(source, wk.object),
                Relation::No(_)
            ),
            "primitive {source:?} must not satisfy object",
        );
        assert!(
            Relater::new(interner.store(), wk)
                .is_assignable(source, empty)
                .is_yes(),
            "non-nullish primitive {source:?} must satisfy {{}}",
        );
    }
    for source in [wk.null, wk.undefined] {
        assert!(matches!(
            Relater::new(interner.store(), wk).is_assignable(source, wk.object),
            Relation::No(_)
        ));
        assert!(matches!(
            Relater::new(interner.store(), wk).is_assignable(source, empty),
            Relation::No(_)
        ));
    }
    assert!(matches!(
        Relater::new(interner.store(), wk).is_assignable(wk.number, object_thenable),
        Relation::No(_)
    ));
}

#[test]
fn template_literal_type_satisfies_empty_structural_object() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let empty = interner.intern_object(ObjectType::default());
    let template = interner.intern_template(crate::types::repr::TemplateType {
        texts: vec!["id:".to_string(), String::new()],
        holes: vec![wk.number],
    });
    assert!(Relater::new(interner.store(), wk)
        .is_assignable(template, empty)
        .is_yes());
}

/// M18 tuple assignability is positional and same-length; length mismatches are
/// terminal, and the first element mismatch is the single nested reason.
#[test]
fn tuple_positional_and_length_assignability() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let lit_one = interner.intern_literal(LiteralValue::Number(1.0));
    let lit_x = interner.intern_literal(LiteralValue::String("x".to_string()));

    // [number, string], [string, number] (order swapped), [number] (shorter),
    // and the literal tuples [1, "x"] / ["x", 1].
    let num_str = interner.intern_tuple(vec![wk.number, wk.string]);
    let str_num = interner.intern_tuple(vec![wk.string, wk.number]);
    let num_only = interner.intern_tuple(vec![wk.number]);
    let lit_num_str = interner.intern_tuple(vec![lit_one, lit_x]);
    let lit_str_num = interner.intern_tuple(vec![lit_x, lit_one]);

    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    // Identity.
    assert!(
        rel.is_assignable(num_str, num_str).is_yes(),
        "[number, string] <: [number, string]"
    );

    // Positional widening: [1, "x"] <: [number, string] (each literal widens at
    // its position).
    assert!(
        rel.is_assignable(lit_num_str, num_str).is_yes(),
        "[1, \"x\"] <: [number, string] (positional literal widening)"
    );

    // Positional MISMATCH: ["x", 1] <: [number, string] fails at position 0
    // (string literal `\"x\"` not assignable to `number`) — a SINGLE TupleElement
    // reason at the first failing index, not one per element.
    match rel.is_assignable(lit_str_num, num_str) {
        Relation::No(chain) => match chain.head() {
            Reason::TupleElement { index, because, .. } => {
                assert_eq!(*index, 0, "first failing position is 0");
                assert!(matches!(**because, Reason::Leaf { .. }));
            }
            other => panic!("expected a TupleElement reason, got {other:?}"),
        },
        Relation::Yes => panic!("[\"x\", 1] must NOT be assignable to [number, string]"),
    }

    // Order is significant: [string, number] is NOT assignable to
    // [number, string] (position 0 string→number fails).
    assert!(
        !rel.is_assignable(str_num, num_str).is_yes(),
        "[string, number] is NOT assignable to [number, string] (positional)"
    );

    // LENGTH mismatch: [number] is NOT assignable to [number, string] — a
    // terminal TupleLength reason (too few), and the reverse is too many.
    match rel.is_assignable(num_only, num_str) {
        Relation::No(chain) => assert!(
            matches!(chain.head(), Reason::TupleLength { .. }),
            "expected a TupleLength reason, got {:?}",
            chain.head()
        ),
        Relation::Yes => panic!("[number] must NOT be assignable to [number, string] (length)"),
    }
    match rel.is_assignable(num_str, num_only) {
        Relation::No(chain) => assert!(
            matches!(chain.head(), Reason::TupleLength { .. }),
            "expected a TupleLength reason for the too-many direction"
        ),
        Relation::Yes => panic!("[number, string] must NOT be assignable to [number] (length)"),
    }

    // A tuple is not assignable to a scalar.
    assert!(
        !rel.is_assignable(num_str, wk.number).is_yes(),
        "[number, string] is NOT assignable to number"
    );
}

/// M18 tuple → array: a tuple is assignable to the array of (a supertype of)
/// every element. `[number, string]` <: `(number | string)[]` and <: `unknown[]`;
/// `[number, string]` is NOT <: `number[]` (the `string` element fails); the empty
/// tuple `[]` <: any `T[]`; and array → tuple is NOT assignable (deferred).
#[test]
fn tuple_to_array_assignability() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    let num_str_tuple = interner.intern_tuple(vec![wk.number, wk.string]);
    let empty_tuple = interner.intern_tuple(vec![]);
    let num_union = interner.union(vec![wk.number, wk.string]);
    let union_arr = interner.intern_array(num_union);
    let num_arr = interner.intern_array(wk.number);
    let unknown_arr = interner.intern_array(wk.unknown);

    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    // [number, string] <: (number | string)[] (each element lands in the union).
    assert!(
        rel.is_assignable(num_str_tuple, union_arr).is_yes(),
        "[number, string] <: (number | string)[]"
    );
    // [number, string] <: unknown[] (every element <: unknown).
    assert!(
        rel.is_assignable(num_str_tuple, unknown_arr).is_yes(),
        "[number, string] <: unknown[]"
    );

    // [number, string] is NOT <: number[] — the `string` element fails, wrapped
    // as an ArrayElement reason.
    match rel.is_assignable(num_str_tuple, num_arr) {
        Relation::No(chain) => assert!(
            matches!(chain.head(), Reason::ArrayElement { .. }),
            "expected an ArrayElement reason, got {:?}",
            chain.head()
        ),
        Relation::Yes => panic!("[number, string] must NOT be assignable to number[]"),
    }

    // The empty tuple [] is assignable to any T[] (no element can fail).
    assert!(
        rel.is_assignable(empty_tuple, num_arr).is_yes(),
        "[] <: number[]"
    );

    // Array → tuple is NOT assignable (deferred): number[] is not <:
    // [number, string].
    assert!(
        !rel.is_assignable(num_arr, num_str_tuple).is_yes(),
        "number[] is NOT assignable to [number, string] (array → tuple deferred)"
    );
}

/// M32 tuple rest relation: fixed source tuples may satisfy array-rest and tuple-rest
/// targets, including fixed suffixes; tuple-to-array checks the variadic element too.
#[test]
fn tuple_rest_assignability() {
    use crate::types::repr::{TupleRestType, TupleType};

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let number_arr = interner.intern_array(wk.number);

    let source = interner.intern_tuple(vec![wk.string, wk.number, wk.number]);
    let resty = interner.intern_tuple_type(TupleType::with_rest(
        vec![wk.string],
        TupleRestType::new(1, number_arr),
    ));
    let middle = interner.intern_tuple_type(TupleType::with_rest(
        vec![wk.string, wk.boolean],
        TupleRestType::new(1, number_arr),
    ));
    let middle_source = interner.intern_tuple(vec![wk.string, wk.number, wk.boolean]);
    let fixed_rest = interner.intern_tuple(vec![wk.string, wk.number]);
    let with_tail = interner.intern_tuple_type(TupleType::with_rest(
        vec![wk.boolean],
        TupleRestType::new(0, fixed_rest),
    ));
    let with_tail_source = interner.intern_tuple(vec![wk.string, wk.number, wk.boolean]);
    let bad_rest_source = interner.intern_tuple(vec![wk.string, wk.string]);
    let string_or_number = interner.union(vec![wk.string, wk.number]);
    let string_or_number_arr = interner.intern_array(string_or_number);

    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    assert!(rel.is_assignable(source, resty).is_yes());
    assert!(rel.is_assignable(middle_source, middle).is_yes());
    assert!(rel.is_assignable(with_tail_source, with_tail).is_yes());
    assert!(rel.is_assignable(with_tail, with_tail_source).is_yes());
    assert!(!rel.is_assignable(bad_rest_source, resty).is_yes());
    assert!(!rel.is_assignable(with_tail, fixed_rest).is_yes());

    assert!(rel.is_assignable(resty, string_or_number_arr).is_yes());
    assert!(!rel.is_assignable(resty, number_arr).is_yes());
}

/// M19 index signatures: every governed source property must fit the target
/// index value. Empty objects fit string indexes, dictionaries compare index
/// values, and number indexes govern only numeric-named members.
#[test]
fn index_signature_assignability() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    // Targets: `{ [k: string]: number }` and `{ [i: number]: string }`.
    let str_dict_num = interner.intern_object(ObjectType {
        string_index: Some(wk.number),
        ..Default::default()
    });
    let num_dict_str = interner.intern_object(ObjectType {
        number_index: Some(wk.string),
        ..Default::default()
    });

    // Sources.
    // `{ a: number; b: number }` — all values fit the string index (number).
    let ab_num = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number), prop("b", wk.number)],
        ..Default::default()
    });
    // `{ a: number; b: string }` — `b` (string) does NOT fit `number`.
    let ab_mixed = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number), prop("b", wk.string)],
        ..Default::default()
    });
    // The empty object `{}`.
    let empty = interner.intern_object(ObjectType::default());
    // A string dictionary `{ [k: string]: string }`.
    let str_dict_str = interner.intern_object(ObjectType {
        string_index: Some(wk.string),
        ..Default::default()
    });
    // A numeric-named member object `{ 0: string }` (its name is numeric).
    let numeric_member = interner.intern_object(ObjectType {
        properties: vec![prop("0", wk.string)],
        ..Default::default()
    });
    // A number dictionary whose value is `number` (for the bad-value direction).
    let num_dict_num = interner.intern_object(ObjectType {
        number_index: Some(wk.number),
        ..Default::default()
    });
    // An object with a **non-numeric** named member `{ a: string }` — a number
    // index signature must NOT constrain it (it is a pure string key).
    let a_str = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.string)],
        ..Default::default()
    });

    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    // OK: every value fits the string index value type.
    assert!(
        rel.is_assignable(ab_num, str_dict_num).is_yes(),
        "{{ a: number; b: number }} <: {{ [k: string]: number }}"
    );
    // OK: the empty object trivially fits (no value can fail).
    assert!(
        rel.is_assignable(empty, str_dict_num).is_yes(),
        "{{}} <: {{ [k: string]: number }}"
    );

    // BAD value: `b: string` is not assignable to the index value type `number`
    // — one `IndexSignature` reason wrapping the leaf cause.
    match rel.is_assignable(ab_mixed, str_dict_num) {
        Relation::No(chain) => match chain.head() {
            Reason::IndexSignature { because, .. } => {
                assert!(matches!(**because, Reason::Leaf { .. }));
            }
            other => panic!("expected an IndexSignature reason, got {other:?}"),
        },
        Relation::Yes => {
            panic!("{{ a: number; b: string }} must NOT be assignable to {{ [k: string]: number }}")
        }
    }

    // Dictionary → dictionary: identity holds; a string dict is NOT assignable to
    // a number dict (its index value `string` does not fit `number`).
    assert!(
        rel.is_assignable(str_dict_num, str_dict_num).is_yes(),
        "{{ [k: string]: number }} <: itself"
    );
    assert!(
        !rel.is_assignable(str_dict_str, str_dict_num).is_yes(),
        "{{ [k: string]: string }} is NOT assignable to {{ [k: string]: number }}"
    );

    // Number index target governs numeric-named members: `{ 0: string }` fits
    // `{ [i: number]: string }`.
    assert!(
        rel.is_assignable(numeric_member, num_dict_str).is_yes(),
        "{{ 0: string }} <: {{ [i: number]: string }}"
    );
    // ...but the numeric member's `string` value does NOT fit a number→number
    // dict — `{ 0: string }` is NOT assignable to `{ [i: number]: number }`.
    assert!(
        !rel.is_assignable(numeric_member, num_dict_num).is_yes(),
        "{{ 0: string }} is NOT assignable to {{ [i: number]: number }} (value mismatch)"
    );
    // A **non-numeric** named member is untouched by a number index signature:
    // `{ a: string }` is assignable to `{ [i: number]: number }` (the `a` key is
    // a pure string key, not governed by the number index).
    assert!(
        rel.is_assignable(a_str, num_dict_num).is_yes(),
        "a non-numeric member is not constrained by a number index signature"
    );
}

/// Exhaustively check the M0 intrinsic-lattice + literal-widening rules so a
/// regression in the relation engine is caught independent of the parser and
/// the fixtures.
#[test]
fn intrinsic_lattice_and_widening() {
    let mut interner = Interner::with_intrinsics();
    // Literal sources used by the M0 fixtures.
    let lit_num = interner.intern_literal(LiteralValue::Number(1.0));
    let lit_str = interner.intern_literal(LiteralValue::String("x".to_string()));
    let lit_bool = interner.intern_literal(LiteralValue::Boolean(true));

    let wk = interner.well_known();
    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    // Literal -> base widening (assignability uses the literal type).
    assert!(rel.is_assignable(lit_num, wk.number).is_yes());
    assert!(rel.is_assignable(lit_str, wk.string).is_yes());
    assert!(rel.is_assignable(lit_bool, wk.boolean).is_yes());
    // Cross-base widening fails.
    assert!(!rel.is_assignable(lit_str, wk.number).is_yes());
    assert!(!rel.is_assignable(lit_num, wk.string).is_yes());
    assert!(!rel.is_assignable(lit_num, wk.boolean).is_yes());
    assert!(!rel.is_assignable(lit_bool, wk.number).is_yes());

    // any: assignable both directions.
    assert!(rel.is_assignable(lit_num, wk.any).is_yes());
    assert!(rel.is_assignable(wk.any, wk.number).is_yes());

    // unknown: top type. Everything -> unknown; unknown -> only unknown/any.
    assert!(rel.is_assignable(lit_num, wk.unknown).is_yes());
    assert!(rel.is_assignable(wk.unknown, wk.unknown).is_yes());
    assert!(rel.is_assignable(wk.unknown, wk.any).is_yes());
    assert!(!rel.is_assignable(wk.unknown, wk.number).is_yes());

    // never: bottom type. never -> anything; nothing -> never except never.
    assert!(rel.is_assignable(wk.never, wk.number).is_yes());
    assert!(rel.is_assignable(wk.never, wk.never).is_yes());
    assert!(!rel.is_assignable(lit_num, wk.never).is_yes());

    // void: accepts undefined and itself.
    assert!(rel.is_assignable(wk.undefined, wk.void).is_yes());
    assert!(rel.is_assignable(wk.void, wk.void).is_yes());
    assert!(!rel.is_assignable(lit_num, wk.void).is_yes());

    // strictNullChecks: null/undefined distinct, each only to self/any/unknown
    // (undefined also to void).
    assert!(rel.is_assignable(wk.null, wk.null).is_yes());
    assert!(rel.is_assignable(wk.undefined, wk.undefined).is_yes());
    assert!(!rel.is_assignable(wk.null, wk.number).is_yes());
    assert!(!rel.is_assignable(wk.undefined, wk.string).is_yes());
    assert!(!rel.is_assignable(wk.undefined, wk.null).is_yes());
    assert!(!rel.is_assignable(wk.null, wk.undefined).is_yes());
}

/// A failure returns a reason chain whose root is the (src, tgt) pair — the
/// hook M6 grows into nested messages, and the data M0's renderer consumes.
#[test]
fn failure_carries_reason_root() {
    let mut interner = Interner::with_intrinsics();
    let lit_str = interner.intern_literal(LiteralValue::String("x".to_string()));
    let wk = interner.well_known();
    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    match rel.is_assignable(lit_str, wk.number) {
        Relation::No(chain) => {
            assert_eq!(chain.root(), (lit_str, wk.number));
        }
        Relation::Yes => panic!("expected a relation failure"),
    }
}

/// The cache returns a stable verdict on repeat queries (smoke test of the
/// 3-`u32` cache path; the cycle stack never fires for non-recursive types).
#[test]
fn repeated_query_is_stable() {
    let mut interner = Interner::with_intrinsics();
    let lit_num = interner.intern_literal(LiteralValue::Number(1.0));
    let wk = interner.well_known();
    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    let first = rel.is_assignable(lit_num, wk.string).is_yes();
    let second = rel.is_assignable(lit_num, wk.string).is_yes();
    assert_eq!(first, second);
    assert!(!first);
}

/// M5 — relating recursive types **terminates** via the assume-true cycle
/// stack (§6.3). This guards the cycle fixpoint: a stack overflow here is the
/// failure mode the recursive-type fixture must never hit. It covers the three
/// paths that each loop forever without the fix:
///
///  1. a recursive interface relating to **itself**,
///  2. a recursive interface relating to a **structural copy** of itself,
///  3. a **failing** recursive relation queried **twice** — the second query
///     hits the cached-false rebuild path, which must recompute under stack
///     protection rather than recurse on the cached failure forever.
#[test]
fn recursive_relation_terminates() {
    use crate::types::repr::ObjectType;

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    // `interface List { head: number; tail: List | null }` — a nominal,
    // self-referential object built reserve-then-fill (its `tail` references
    // its own id, never an inlined expansion).
    let list = interner.reserve_object();
    let list_or_null = interner.union(vec![list, wk.null]);
    interner.fill_object(
        list,
        ObjectType {
            properties: vec![prop("head", wk.number), prop("tail", list_or_null)],
            ..Default::default()
        },
    );

    // A *structural* copy with the same shape but its own id (built the same
    // way so it too is self-referential).
    let copy = interner.reserve_object();
    let copy_or_null = interner.union(vec![copy, wk.null]);
    interner.fill_object(
        copy,
        ObjectType {
            properties: vec![prop("head", wk.number), prop("tail", copy_or_null)],
            ..Default::default()
        },
    );

    // Two mutually-shaped recursive interfaces that DISAGREE at a leaf:
    // `A { self: A; tag: number }` vs `B { self: B; tag: string }`. Relating
    // `A <: B` re-enters `(A, B)` via `self` (assumed true) but fails on `tag`.
    let a = interner.reserve_object();
    let b = interner.reserve_object();
    interner.fill_object(
        a,
        ObjectType {
            properties: vec![prop("self", a), prop("tag", wk.number)],
            ..Default::default()
        },
    );
    interner.fill_object(
        b,
        ObjectType {
            properties: vec![prop("self", b), prop("tag", wk.string)],
            ..Default::default()
        },
    );

    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    // 1. Recursive interface relates to itself (identity short-circuit, but the
    //    union member `List | null` still gets relate'd through the cycle for
    //    `List <: List | null`).
    assert!(rel.is_assignable(list, list).is_yes(), "List <: List");
    assert!(
        rel.is_assignable(list_or_null, list_or_null).is_yes(),
        "List | null <: List | null"
    );

    // 2. Recursive interface relates to a structural copy — must terminate and
    //    succeed (each side's `tail` re-enters the in-flight `(List, copy)`).
    assert!(
        rel.is_assignable(list, copy).is_yes(),
        "List <: structural copy must terminate as success"
    );
    assert!(
        rel.is_assignable(copy, list).is_yes(),
        "structural copy <: List must terminate as success"
    );

    // 3. The failing recursive relation, queried TWICE. The first decides
    //    false (and caches the bool); the second recomputes the reason on the
    //    cached-false path — which must run under stack protection and
    //    terminate, not loop. The leaf cause is the `tag` mismatch.
    let first = rel.is_assignable(a, b);
    assert!(!first.is_yes(), "A is not assignable to B (tag mismatch)");
    let second = rel.is_assignable(a, b);
    assert!(
        !second.is_yes(),
        "repeat query of a failing recursive relation must terminate with the same verdict"
    );
    // The rebuilt reason still points at the offending `tag` property.
    match second {
        Relation::No(chain) => match chain.head() {
            Reason::Property { name, .. } => assert_eq!(name, "tag"),
            other => panic!("expected the `tag` Property failure, got {other:?}"),
        },
        Relation::Yes => unreachable!(),
    }
}

/// M31 — the merged-source recursion over a **recursive object target** must
/// TERMINATE (the finding-1 fix's blocker: an unguarded per-property recursion
/// stack-overflows on a self-referential property). Covers the three paths:
///
///  1. a **single-contributor** recursive property (`Rec & { extra } <: Rec`,
///     `Rec { next: Rec }`) — delegates to the cycle-guarded main engine (Part A);
///  2. a **multi-contributor** recursive property (`P & Q <: P`, `P { p: P }` /
///     `Q { p: Q }`) — the coinductive assume-true `in_flight` guard (Part B); tsc
///     accepts this fixpoint, so it must stay CLEAN, not over-report;
///  3. a recursive target with a leaf **mismatch** the walk must still REACH and
///     report (`Cell & { extra } <: { next: Cell; value: string }`).
#[test]
fn merged_intersection_source_recurses_to_a_fixpoint() {
    use crate::types::repr::ObjectType;

    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    // `interface Rec { next: Rec }` — self-referential (reserve-then-fill).
    let rec = interner.reserve_object();
    interner.fill_object(
        rec,
        ObjectType {
            properties: vec![prop("next", rec)],
            ..Default::default()
        },
    );
    let extra = interner.intern_object(ObjectType {
        properties: vec![prop("extra", wk.number)],
        ..Default::default()
    });
    // `Rec & { extra: number }`.
    let rec_and_extra = interner.intersection(vec![rec, extra]);

    // `interface Cell { next: Cell; value: number }` and a target that wants
    // `value: string` (a leaf mismatch behind the recursion).
    let cell = interner.reserve_object();
    interner.fill_object(
        cell,
        ObjectType {
            properties: vec![prop("next", cell), prop("value", wk.number)],
            ..Default::default()
        },
    );
    let extra_str = interner.intern_object(ObjectType {
        properties: vec![prop("extra", wk.string)],
        ..Default::default()
    });
    let cell_and_extra = interner.intersection(vec![cell, extra_str]);
    let cell_wrong = interner.intern_object(ObjectType {
        properties: vec![prop("next", cell), prop("value", wk.string)],
        ..Default::default()
    });

    // `interface P { p: P }` / `interface Q { p: Q }` — both contribute the
    // recursive key `p`, so `P & Q <: P` exercises the multi-contributor guard.
    let p = interner.reserve_object();
    let q = interner.reserve_object();
    interner.fill_object(
        p,
        ObjectType {
            properties: vec![prop("p", p)],
            ..Default::default()
        },
    );
    interner.fill_object(
        q,
        ObjectType {
            properties: vec![prop("p", q)],
            ..Default::default()
        },
    );
    let p_and_q = interner.intersection(vec![p, q]);

    let store = interner.store();
    let mut rel = Relater::new(store, wk);

    // 1. Single-contributor recursive property — must terminate as success.
    assert!(
        rel.is_assignable(rec_and_extra, rec).is_yes(),
        "Rec & {{ extra }} <: Rec must terminate (single-contributor Part A)"
    );

    // 2. Multi-contributor recursive property — coinductive fixpoint, stays clean.
    assert!(
        rel.is_assignable(p_and_q, p).is_yes(),
        "P & Q <: P must terminate as success (multi-contributor Part B)"
    );

    // 3. The leaf mismatch behind the recursion must still be reached + reported.
    match rel.is_assignable(cell_and_extra, cell_wrong) {
        Relation::No(chain) => match chain.head() {
            Reason::Property { name, .. } => assert_eq!(name, "value"),
            other => panic!("expected the `value` Property failure, got {other:?}"),
        },
        Relation::Yes => panic!("the `value: number` vs `value: string` mismatch must be caught"),
    }
}

/// M5 soundness — a recursive **false** verdict must be **order-independent**:
/// it must not depend on whether an enclosing assume-true query ran first
/// (architecture §6.3). This is the cache-poisoning hazard: relating `CA <: AA`
/// decides the nested `CB <: AB` *provisionally* true under the in-flight
/// `(CA, AA)` assumption; if that provisional `true` were committed to the
/// durable cache, a later INDEPENDENT `CB <: AB` query would read it as ground
/// truth and drop a real error. The fix never caches a verdict that rested on an
/// assumption about an ancestor key, so the standalone verdict is identical
/// either way.
#[test]
fn recursive_false_verdict_is_order_independent() {
    use crate::types::repr::ObjectType;

    // Build the four mutually-recursive interfaces once; reused for both orders.
    // AA { peer: AB; tag: number }   AB { back: AA; leaf: number }
    // CA { peer: CB; tag: string }   CB { back: CA; leaf: number }  (tag differs)
    fn build(interner: &mut Interner) -> (TypeId, TypeId, TypeId, TypeId) {
        let wk = interner.well_known();
        let (aa, ab, ca, cb) = (
            interner.reserve_object(),
            interner.reserve_object(),
            interner.reserve_object(),
            interner.reserve_object(),
        );
        interner.fill_object(
            aa,
            ObjectType {
                properties: vec![prop("peer", ab), prop("tag", wk.number)],
                ..Default::default()
            },
        );
        interner.fill_object(
            ab,
            ObjectType {
                properties: vec![prop("back", aa), prop("leaf", wk.number)],
                ..Default::default()
            },
        );
        interner.fill_object(
            ca,
            ObjectType {
                properties: vec![prop("peer", cb), prop("tag", wk.string)],
                ..Default::default()
            },
        );
        interner.fill_object(
            cb,
            ObjectType {
                properties: vec![prop("back", ca), prop("leaf", wk.number)],
                ..Default::default()
            },
        );
        (aa, ab, ca, cb)
    }

    // Order A: query `CA <: AA` FIRST (which provisionally relates `CB <: AB`
    // under the `(CA, AA)` assumption), THEN the standalone `CB <: AB`.
    let order_a_cb_ab = {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let (aa, ab, ca, cb) = build(&mut interner);
        let store = interner.store();
        let mut rel = Relater::new(store, wk);
        // The enclosing query: genuinely false (top-level `tag` mismatch).
        assert!(
            !rel.is_assignable(ca, aa).is_yes(),
            "CA <: AA is false (tag)"
        );
        // The standalone nested query MUST still be false.
        rel.is_assignable(cb, ab).is_yes()
    };

    // Order B: the standalone `CB <: AB` query alone, no enclosing query.
    let order_b_cb_ab = {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let (_aa, ab, _ca, cb) = build(&mut interner);
        let store = interner.store();
        let mut rel = Relater::new(store, wk);
        rel.is_assignable(cb, ab).is_yes()
    };

    // The verdict is the same either way, and it is FALSE: `CB <: AB` requires
    // `CB.back (CA) <: AB.back (AA)`, which fails on the `tag` leaf.
    assert_eq!(
        order_a_cb_ab, order_b_cb_ab,
        "a recursive false verdict must not depend on an enclosing assume-true query"
    );
    assert!(
        !order_b_cb_ab,
        "CB is not assignable to AB (the recursive `tag` mismatch must be reported)"
    );
}

/// M27 — template **patterns** in the relation engine: a string literal matches a
/// pattern by anchored segment scanning (`${string}` any, `${number}` a decimal), a
/// pattern flows into `string` and into a subsuming pattern, and `string` matches only
/// the bare `` `${string}` `` hole.
#[test]
fn template_pattern_assignability() {
    use crate::types::repr::TemplateType;
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    let template = |interner: &mut Interner, texts: &[&str], holes: Vec<TypeId>| {
        interner.intern_template(TemplateType {
            texts: texts.iter().map(|t| t.to_string()).collect(),
            holes,
        })
    };
    let s = |interner: &mut Interner, v: &str| {
        interner.intern_literal(LiteralValue::String(v.to_string()))
    };

    // Patterns.
    let greeting = template(&mut interner, &["hello ", ""], vec![wk.string]); // `hello ${string}`
    let num_hole = template(&mut interner, &["n", ""], vec![wk.number]); // `n${number}`
    let bare = template(&mut interner, &["", ""], vec![wk.string]); // `${string}`
    let h_hole = template(&mut interner, &["h", ""], vec![wk.string]); // `h${string}`
    let x_hole = template(&mut interner, &["x", ""], vec![wk.string]); // `x${string}`

    // Literal sources.
    let hello_world = s(&mut interner, "hello world");
    let goodbye = s(&mut interner, "goodbye world");
    let n42 = s(&mut interner, "n42");
    let n35 = s(&mut interner, "n3.5");
    let nx = s(&mut interner, "nx");

    let yes = |interner: &Interner, src: TypeId, tgt: TypeId| {
        let mut rel = Relater::new(interner.store(), wk);
        rel.is_assignable(src, tgt).is_yes()
    };

    // Literal → pattern (anchored matching).
    assert!(
        yes(&interner, hello_world, greeting),
        "\"hello world\" <: `hello ${{string}}`"
    );
    assert!(
        !yes(&interner, goodbye, greeting),
        "\"goodbye world\" not <: `hello ${{string}}`"
    );
    assert!(yes(&interner, n42, num_hole), "\"n42\" <: `n${{number}}`");
    assert!(yes(&interner, n35, num_hole), "\"n3.5\" <: `n${{number}}`");
    assert!(
        !yes(&interner, nx, num_hole),
        "\"nx\" not <: `n${{number}}` (non-numeric)"
    );

    // `string` → pattern: only the bare `${string}` hole.
    assert!(
        !yes(&interner, wk.string, greeting),
        "string not <: `hello ${{string}}`"
    );
    assert!(yes(&interner, wk.string, bare), "string <: `${{string}}`");

    // Pattern → string, and pattern subsumption.
    assert!(
        yes(&interner, greeting, wk.string),
        "`hello ${{string}}` <: string"
    );
    assert!(
        yes(&interner, greeting, bare),
        "`hello ${{string}}` <: `${{string}}`"
    );
    assert!(
        yes(&interner, greeting, h_hole),
        "`hello ${{string}}` <: `h${{string}}`"
    );
    assert!(
        !yes(&interner, greeting, x_hole),
        "`hello ${{string}}` not <: `x${{string}}`"
    );
}

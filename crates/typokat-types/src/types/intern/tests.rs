use super::*;
use crate::types::repr::{
    ClassId, ConditionalType, FunctionType, GenericTypeParam, LiteralValue, MappedType, ModifierOp,
    ObjectType, ParameterType, PropertyKey, PropertyType, TemplateType, TupleRestType, TupleType,
    TypeParamId, TypeTag, WellKnownSymbol,
};

/// Build a required public property `name: ty`.
fn prop(name: &str, ty: TypeId) -> PropertyType {
    PropertyType::public(name, ty)
}

#[test]
fn symbol_property_identity_survives_interning_freeze_and_reserved_fill() {
    let mut interner = Interner::with_intrinsics();
    let number = interner.well_known().number;
    let string_spelling = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("Symbol.iterator", number)],
        ..Default::default()
    });
    let iterator = interner.intern_object(ObjectType {
        properties: vec![PropertyType::well_known_symbol(
            WellKnownSymbol::Iterator,
            number,
        )],
        ..Default::default()
    });
    let async_iterator = interner.intern_object(ObjectType {
        properties: vec![PropertyType::well_known_symbol(
            WellKnownSymbol::AsyncIterator,
            number,
        )],
        ..Default::default()
    });
    assert_ne!(string_spelling, iterator);
    assert_ne!(iterator, async_iterator);
    let iterator_object = interner
        .store()
        .object_type(iterator)
        .expect("iterator object is stored");
    assert!(iterator_object.property("Symbol.iterator").is_none());
    assert!(iterator_object
        .property_by_key(&PropertyKey::WellKnownSymbol(WellKnownSymbol::Iterator))
        .is_some());

    let reordered = interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::well_known_symbol(WellKnownSymbol::AsyncIterator, number),
            PropertyType::well_known_symbol(WellKnownSymbol::Iterator, number),
        ],
        ..Default::default()
    });
    let canonical = interner.intern_object(ObjectType {
        properties: vec![
            PropertyType::well_known_symbol(WellKnownSymbol::Iterator, number),
            PropertyType::well_known_symbol(WellKnownSymbol::AsyncIterator, number),
        ],
        ..Default::default()
    });
    assert_eq!(reordered, canonical);

    let reserved = interner.reserve_object();
    interner
        .fill_reserved_type_batch(vec![ReservedTypeFill::Object(
            reserved,
            ObjectType {
                properties: vec![PropertyType::well_known_symbol(
                    WellKnownSymbol::Iterator,
                    number,
                )],
                ..Default::default()
            },
        )])
        .expect("symbol-keyed reservation fills");
    assert_eq!(
        interner
            .store()
            .object_type(reserved)
            .and_then(|object| object.properties.first())
            .and_then(|property| property.key.as_well_known_symbol()),
        Some(WellKnownSymbol::Iterator)
    );

    interner
        .freeze_as_base()
        .expect("complete symbol base seals");
    let mut delta = interner.fork_delta().expect("symbol base forks");
    assert_eq!(
        delta.intern_object(ObjectType {
            properties: vec![PropertyType::well_known_symbol(
                WellKnownSymbol::Iterator,
                number,
            )],
            ..Default::default()
        }),
        iterator
    );
    assert_eq!(
        delta
            .store()
            .object_type(iterator)
            .and_then(|object| object.properties.first())
            .map(|property| &property.key),
        Some(&PropertyKey::WellKnownSymbol(WellKnownSymbol::Iterator))
    );
}

struct ColdFamilyRows {
    rows: Vec<(TypeId, TypeTag)>,
    literal: TypeId,
    array: TypeId,
    template: TypeId,
    type_parameter: TypeId,
    type_parameter_id: TypeParamId,
}

fn populate_every_cold_family(
    interner: &mut Interner,
    label: &str,
    identity: u32,
) -> ColdFamilyRows {
    let wk = interner.well_known();
    let literal = interner.intern_literal(LiteralValue::String(label.to_owned()));
    let object = interner.intern_object(ObjectType {
        properties: vec![prop(label, literal)],
        ..Default::default()
    });
    let union = interner.union(vec![literal, wk.number]);
    let function = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required(label, literal)],
        ret: object,
    });
    let type_parameter_id = TypeParamId(identity);
    let type_parameter = interner.intern_type_param(type_parameter_id, label);
    let array = interner.intern_array(literal);
    let tuple = interner.intern_tuple(vec![literal, array]);
    let intersection = interner.intersection(vec![object, array]);
    let conditional = interner.intern_conditional(ConditionalType {
        check: type_parameter,
        extends_ty: wk.string,
        true_branch: object,
        false_branch: wk.never,
        infer_count: 0,
        distributive: true,
        poisoned: false,
    });
    let instantiation =
        interner.intern_instantiation(conditional, vec![(type_parameter_id, literal)]);
    let class_instance = interner.intern_class_instance(ClassId(identity), vec![literal, object]);
    let deferred = interner.intern_deferred_indexed_access(class_instance, literal);
    let mapped = interner.intern_mapped(MappedType {
        homomorphic: false,
        key_source: literal,
        value_template: object,
        modifiers_source: Some(array),
        optional_modifier: ModifierOp::Keep,
        readonly_modifier: ModifierOp::Add,
    });
    let template = interner.intern_template(TemplateType {
        texts: vec![label.to_owned(), String::new()],
        holes: vec![literal],
    });

    ColdFamilyRows {
        rows: vec![
            (literal, TypeTag::Literal),
            (object, TypeTag::Object),
            (union, TypeTag::Union),
            (function, TypeTag::Function),
            (type_parameter, TypeTag::TypeParam),
            (array, TypeTag::Array),
            (tuple, TypeTag::Tuple),
            (intersection, TypeTag::Intersection),
            (conditional, TypeTag::Conditional),
            (instantiation, TypeTag::Instantiation),
            (class_instance, TypeTag::ClassInstance),
            (deferred, TypeTag::DeferredIndexedAccess),
            (mapped, TypeTag::Mapped),
            (template, TypeTag::Template),
        ],
        literal,
        array,
        template,
        type_parameter,
        type_parameter_id,
    }
}

fn assert_every_cold_family_is_readable(store: &Store, rows: &[(TypeId, TypeTag)]) {
    for &(id, expected_tag) in rows {
        assert_eq!(store.tag(id), expected_tag);
        let present = match expected_tag {
            TypeTag::Literal => store.literal_value(id).is_some(),
            TypeTag::Object => store.object_type(id).is_some(),
            TypeTag::Union => store.union_members(id).is_some(),
            TypeTag::Intersection => store.intersection_members(id).is_some(),
            TypeTag::Function => store.function_type(id).is_some(),
            TypeTag::TypeParam => store.type_param(id).is_some(),
            TypeTag::Array => store.array_type(id).is_some(),
            TypeTag::Tuple => store.tuple_type(id).is_some(),
            TypeTag::Conditional => store.conditional_type(id).is_some(),
            TypeTag::Instantiation => store.instantiation_type(id).is_some(),
            TypeTag::ClassInstance => store.class_instance_type(id).is_some(),
            TypeTag::DeferredIndexedAccess => store.deferred_indexed_access_type(id).is_some(),
            TypeTag::Mapped => store.mapped_type(id).is_some(),
            TypeTag::Template => store.template_type(id).is_some(),
            _ => false,
        };
        assert!(present, "{expected_tag:?} cold payload is readable");
    }
}

/// Hash-consing: structurally identical types share one `TypeId`
/// (architecture §3, mvp-plan §6 — a correctness-critical invariant).
#[test]
fn hash_consing_dedups_intrinsics_and_literals() {
    let mut interner = Interner::with_intrinsics();

    // Re-interning an intrinsic returns the same well-known id.
    let number_again = interner.intern_intrinsic(IntrinsicKind::Number);
    assert_eq!(number_again, interner.well_known().number);

    // Equal literals collapse to one id; different literals do not.
    let a = interner.intern_literal(LiteralValue::Number(1.0));
    let b = interner.intern_literal(LiteralValue::Number(1.0));
    let c = interner.intern_literal(LiteralValue::Number(2.0));
    assert_eq!(a, b, "equal number literals must share an id");
    assert_ne!(a, c, "distinct number literals must not share an id");

    // Equal strings collapse; a string literal is distinct from a number
    // literal even if numerically suggestive.
    let s1 = interner.intern_literal(LiteralValue::String("x".to_string()));
    let s2 = interner.intern_literal(LiteralValue::String("x".to_string()));
    assert_eq!(s1, s2, "equal string literals must share an id");
    assert_ne!(s1, a, "string and number literals must be distinct types");

    // Booleans dedup per value.
    let t1 = interner.intern_literal(LiteralValue::Boolean(true));
    let t2 = interner.intern_literal(LiteralValue::Boolean(true));
    let f1 = interner.intern_literal(LiteralValue::Boolean(false));
    assert_eq!(t1, t2);
    assert_ne!(t1, f1);
}

#[test]
fn sealed_base_forks_share_prefix_and_isolate_dense_suffixes() {
    let mut base = Interner::with_intrinsics();
    let wk = base.well_known();
    let base_array = base.intern_array(wk.string);
    let base_literal = base.intern_literal(LiteralValue::String("base".to_owned()));
    let base_parameter_id = TypeParamId(90_001);
    let base_parameter = base.intern_type_param(base_parameter_id, "T");
    assert_eq!(base.store().type_param_name(base_parameter_id), Some("T"));
    assert_eq!(base.store().type_param_name(TypeParamId(90_999)), None);
    assert!(base.set_type_param_constraint(base_parameter_id, wk.string));

    let base_reserved = base.reserve_object();
    base.fill_reserved_type_batch(vec![ReservedTypeFill::Object(
        base_reserved,
        ObjectType::default(),
    )])
    .expect("base reservation fills before sealing");

    let standalone_references = base.reference_records_for_test();
    let prefix_len = base.store().len();
    let prefix_id = u32::try_from(prefix_len).expect("test prefix fits TypeId");
    let old_graph = Arc::clone(base.store().semantic_graph_identity());
    base.freeze_as_base().expect("complete interner seals");
    assert_eq!(
        base.reference_records_for_test(),
        standalone_references,
        "sealing a complete interner does not move a single identity edge"
    );

    let mut first = base.fork_delta().expect("first private suffix forks");
    let mut second = base.fork_delta().expect("second private suffix forks");
    assert!(first.store().shares_base_rows_with(second.store()));
    assert!(first.shares_base_indexes_with(&second));
    assert_eq!(first.store().type_param_name(base_parameter_id), Some("T"));
    assert_eq!(second.store().type_param_name(base_parameter_id), Some("T"));
    assert_eq!(
        first.intern_type_param(base_parameter_id, "RenamedBase"),
        base_parameter
    );
    assert_eq!(
        first.store().type_param_name(base_parameter_id),
        Some("T"),
        "the exact index preserves the first rendering name"
    );
    assert!(!Arc::ptr_eq(
        first.store().semantic_graph_identity(),
        &old_graph
    ));
    assert!(!Arc::ptr_eq(
        first.store().semantic_graph_identity(),
        second.store().semantic_graph_identity()
    ));

    assert_eq!(first.intern_array(wk.string), base_array);
    assert_eq!(
        first.intern_literal(LiteralValue::String("base".to_owned())),
        base_literal
    );
    assert_eq!(
        first
            .store()
            .array_type(base_array)
            .map(|array| array.element),
        Some(wk.string)
    );

    let first_local = first.intern_literal(LiteralValue::String("first".to_owned()));
    assert_eq!(first_local, TypeId(prefix_id));
    assert_eq!(
        first.intern_literal(LiteralValue::String("first".to_owned())),
        first_local,
        "equal suffix identities deduplicate locally"
    );
    let next_local = first.intern_array(first_local);
    assert_eq!(next_local, TypeId(prefix_id + 1));
    let suffix_parameter_id = TypeParamId(900_001);
    let suffix_parameter = first.intern_type_param(suffix_parameter_id, "Suffix");
    assert_eq!(
        first.store().type_param_name(suffix_parameter_id),
        Some("Suffix")
    );
    assert_eq!(
        first.intern_type_param(suffix_parameter_id, "RenamedSuffix"),
        suffix_parameter
    );
    assert_eq!(
        first.store().type_param_name(suffix_parameter_id),
        Some("Suffix")
    );
    assert_eq!(second.store().type_param_name(suffix_parameter_id), None);
    assert_eq!(first.store().type_param_name(TypeParamId(u32::MAX)), None);
    assert_eq!(
        second.store().len(),
        prefix_len,
        "forks cannot observe siblings"
    );

    let second_local = second.intern_literal(LiteralValue::String("second".to_owned()));
    assert_eq!(second_local, TypeId(prefix_id));
    assert_eq!(
        second.store().literal_value(second_local),
        Some(&LiteralValue::String("second".to_owned()))
    );
    assert_eq!(
        first.store().literal_value(first_local),
        Some(&LiteralValue::String("first".to_owned()))
    );

    assert!(matches!(
        first.fill_reserved_type_batch(vec![ReservedTypeFill::Object(
            base_reserved,
            ObjectType::default(),
        )]),
        Err(ReservedTypeFillError::AlreadyFrozen(id)) if id == base_reserved
    ));
    assert!(!first.set_type_param_constraint(base_parameter_id, wk.number));
    assert_eq!(
        first.store().type_param_constraint(base_parameter_id),
        Some(wk.string)
    );
    assert!(first.strict_terminal_state_for_test().is_err());
}

#[test]
fn sealing_rejects_pending_reservations_and_nonempty_suffix_reforks() {
    let mut unfinished = Interner::with_intrinsics();
    let _pending = unfinished.reserve_object();
    assert!(unfinished.freeze_as_base().is_err());

    let mut base = Interner::with_intrinsics();
    base.freeze_as_base().expect("intrinsic-only base seals");
    let mut delta = base.fork_delta().expect("empty delta forks");
    let _ = delta.intern_literal(LiteralValue::String("local".to_owned()));
    assert!(delta.fork_delta().is_err());
}

#[test]
fn side_column_only_deltas_fail_closed_and_invalid_template_owners_are_rejected() {
    let mut base = Interner::with_intrinsics();
    base.freeze_as_base().expect("intrinsic-only base seals");

    let mut invalid_template = base.fork_delta().expect("empty delta forks");
    let missing_row = TypeId(u32::try_from(base.store().len()).expect("test base fits TypeId"));
    invalid_template.set_template_name(missing_row, "not-a-row");
    invalid_template
        .strict_terminal_state_for_test()
        .expect("invalid template metadata owner is rejected before mutation");

    let mut constraint_only = base.fork_delta().expect("constraint delta forks");
    assert!(
        constraint_only.set_type_param_constraint(TypeParamId(900_010), base.well_known().string)
    );
    assert_eq!(constraint_only.store().len(), base.store().len());
    assert!(constraint_only.strict_terminal_state_for_test().is_err());

    let mut frozen_only = base.fork_delta().expect("freeze delta forks");
    frozen_only
        .freeze_type_param_metadata(&[TypeParamId(900_011)])
        .expect("declaration metadata may exist without a TypeParam row");
    assert_eq!(frozen_only.store().len(), base.store().len());
    assert!(frozen_only.strict_terminal_state_for_test().is_err());
}

#[test]
fn every_cold_payload_family_routes_across_nonzero_base_offsets() {
    let mut base = Interner::with_intrinsics();
    let base_rows = populate_every_cold_family(&mut base, "base-cold", 910_000);
    assert_every_cold_family_is_readable(base.store(), &base_rows.rows);

    let base_reserved_object = base.reserve_object();
    let base_reserved_conditional = base.reserve_conditional();
    let base_reserved_mapped = base.reserve_mapped();
    base.fill_reserved_type_batch(vec![
        ReservedTypeFill::Object(base_reserved_object, ObjectType::default()),
        ReservedTypeFill::Conditional(
            base_reserved_conditional,
            ConditionalType {
                check: base_rows.type_parameter,
                extends_ty: base.well_known().string,
                true_branch: base_reserved_object,
                false_branch: base.well_known().never,
                infer_count: 0,
                distributive: true,
                poisoned: false,
            },
        ),
        ReservedTypeFill::Mapped(
            base_reserved_mapped,
            MappedType {
                homomorphic: false,
                key_source: base_rows.literal,
                value_template: base_reserved_object,
                modifiers_source: None,
                optional_modifier: ModifierOp::Keep,
                readonly_modifier: ModifierOp::Keep,
            },
        ),
    ])
    .expect("base reserve batch fills");
    base.freeze_as_base().expect("complete cold base seals");

    let prefix_len = base.store().len();
    let prefix_id = u32::try_from(prefix_len).expect("cold base fits TypeId");
    let mut delta = base.fork_delta().expect("cold delta forks");
    let local_rows = populate_every_cold_family(&mut delta, "local-cold", 920_000);
    assert_every_cold_family_is_readable(delta.store(), &base_rows.rows);
    assert_every_cold_family_is_readable(delta.store(), &local_rows.rows);

    for (offset, &(id, _)) in local_rows.rows.iter().enumerate() {
        let offset = u32::try_from(offset).expect("cold-family table fits TypeId");
        assert_eq!(id, TypeId(prefix_id + offset), "local ids stay dense");
    }
    assert_eq!(delta.intern_array(base_rows.literal), base_rows.array);
    assert_eq!(
        delta.intern_template(TemplateType {
            texts: vec!["local-cold".to_owned(), String::new()],
            holes: vec![local_rows.literal],
        }),
        local_rows.template,
        "equal local cold payload deduplicates"
    );

    assert!(delta.set_type_param_constraint(local_rows.type_parameter_id, base.well_known().number));
    assert_eq!(
        delta
            .store()
            .type_param_constraint(local_rows.type_parameter_id),
        Some(base.well_known().number)
    );

    let reserved_object = delta.reserve_object();
    let reserved_conditional = delta.reserve_conditional();
    let reserved_mapped = delta.reserve_mapped();
    delta.set_template_name(reserved_conditional, "LocalConditional");
    delta.set_template_name(reserved_mapped, "LocalMapped");
    delta
        .fill_reserved_type_batch(vec![
            ReservedTypeFill::Object(
                reserved_object,
                ObjectType {
                    properties: vec![prop("local-reserved", local_rows.literal)],
                    ..Default::default()
                },
            ),
            ReservedTypeFill::Conditional(
                reserved_conditional,
                ConditionalType {
                    check: local_rows.type_parameter,
                    extends_ty: base.well_known().string,
                    true_branch: reserved_object,
                    false_branch: base.well_known().never,
                    infer_count: 0,
                    distributive: true,
                    poisoned: false,
                },
            ),
            ReservedTypeFill::Mapped(
                reserved_mapped,
                MappedType {
                    homomorphic: true,
                    key_source: local_rows.literal,
                    value_template: reserved_object,
                    modifiers_source: Some(local_rows.array),
                    optional_modifier: ModifierOp::Add,
                    readonly_modifier: ModifierOp::Remove,
                },
            ),
        ])
        .expect("nonzero-base local reserve batch fills local side-table offsets");

    assert_eq!(
        delta
            .store()
            .object_type(reserved_object)
            .expect("reserved local object is readable")
            .properties[0]
            .key
            .as_string(),
        Some("local-reserved")
    );
    assert!(delta
        .store()
        .conditional_type(reserved_conditional)
        .is_some());
    assert!(delta.store().mapped_type(reserved_mapped).is_some());
    assert_eq!(
        delta.store().template_name(reserved_conditional),
        Some("LocalConditional")
    );
    assert_eq!(
        delta.store().template_name(reserved_mapped),
        Some("LocalMapped")
    );
    assert!(delta.strict_terminal_state_for_test().is_err());
}

#[test]
fn class_instances_hash_by_class_and_ordered_arguments() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    let a = interner.intern_class_instance(ClassId(1), vec![wk.number, wk.string]);
    let equal = interner.intern_class_instance(ClassId(1), vec![wk.number, wk.string]);
    let other_class = interner.intern_class_instance(ClassId(2), vec![wk.number, wk.string]);
    let reordered = interner.intern_class_instance(ClassId(1), vec![wk.string, wk.number]);

    assert_eq!(a, equal);
    assert_ne!(a, other_class);
    assert_ne!(a, reordered);
    assert_eq!(interner.store().tag(a), TypeTag::ClassInstance);
    assert!(interner.store().instantiation_type(a).is_none());
}

#[test]
fn deferred_indexed_access_hashes_ordered_operands() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    let a = interner.intern_deferred_indexed_access(wk.number, wk.string);
    let equal = interner.intern_deferred_indexed_access(wk.number, wk.string);
    let reordered = interner.intern_deferred_indexed_access(wk.string, wk.number);

    assert_eq!(a, equal);
    assert_ne!(a, reordered);
    assert_eq!(interner.store().tag(a), TypeTag::DeferredIndexedAccess);
}

/// Object hash-consing + canonicalization (mvp-plan §3.3, M2): two object
/// types that differ only in member *order* must collapse to one `TypeId`,
/// while a genuinely different shape (different property type) must not.
#[test]
fn object_canonicalization_dedups_by_member_set() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    // `{ a: number; b: string }` and `{ b: string; a: number }` — same set,
    // different source order — hash-cons to the SAME id.
    let ab = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number), prop("b", wk.string)],
        ..Default::default()
    });
    let ba = interner.intern_object(ObjectType {
        properties: vec![prop("b", wk.string), prop("a", wk.number)],
        ..Default::default()
    });
    assert_eq!(ab, ba, "member order must not affect identity");

    // Re-interning the exact same shape returns the same id.
    let ab_again = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number), prop("b", wk.string)],
        ..Default::default()
    });
    assert_eq!(ab, ab_again);

    // A different property *type* is a distinct object type.
    let ab_diff = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.string), prop("b", wk.string)],
        ..Default::default()
    });
    assert_ne!(ab, ab_diff, "differing property types must not dedup");

    // A different property *set* (extra member) is distinct.
    let abc = interner.intern_object(ObjectType {
        properties: vec![
            prop("a", wk.number),
            prop("b", wk.string),
            prop("c", wk.boolean),
        ],
        ..Default::default()
    });
    assert_ne!(ab, abc, "differing property sets must not dedup");

    // The canonical stored order is key-sorted regardless of input order.
    let stored = interner
        .store()
        .object_type(ba)
        .expect("ba is an object type");
    let names: Vec<&str> = stored
        .properties
        .iter()
        .filter_map(|property| property.key.as_string())
        .collect();
    assert_eq!(names, ["a", "b"], "stored order must be canonical (sorted)");

    // Nested object identity flows through: two outer objects whose nested
    // member is the *same* interned inner id dedup, exercising the by-id
    // property comparison.
    let outer1 = interner.intern_object(ObjectType {
        properties: vec![prop("a", ab)],
        ..Default::default()
    });
    let outer2 = interner.intern_object(ObjectType {
        properties: vec![prop("a", ba)], // ba == ab,
        ..Default::default()
    });
    assert_eq!(outer1, outer2, "nested object identity must propagate");
}

#[test]
fn reserved_type_batch_fills_and_freezes_every_placeholder_kind() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let object = interner.reserve_object();
    let conditional = interner.reserve_conditional();
    let mapped = interner.reserve_mapped();

    let conditional_body = ConditionalType {
        check: wk.number,
        extends_ty: wk.number,
        true_branch: wk.string,
        false_branch: wk.boolean,
        infer_count: 0,
        distributive: false,
        poisoned: false,
    };
    let mapped_body = MappedType {
        homomorphic: false,
        key_source: wk.string,
        value_template: wk.number,
        modifiers_source: None,
        optional_modifier: ModifierOp::Keep,
        readonly_modifier: ModifierOp::Keep,
    };

    assert_eq!(
        interner.fill_reserved_type_batch(vec![
            ReservedTypeFill::Object(
                object,
                ObjectType {
                    properties: vec![prop("z", wk.string), prop("a", wk.number)],
                    ..Default::default()
                },
            ),
            ReservedTypeFill::Conditional(conditional, conditional_body),
            ReservedTypeFill::Mapped(mapped, mapped_body),
        ]),
        Ok(())
    );

    let stored_object = interner
        .store()
        .object_type(object)
        .expect("reserved object must retain its backing row");
    let names: Vec<&str> = stored_object
        .properties
        .iter()
        .filter_map(|property| property.key.as_string())
        .collect();
    assert_eq!(names, ["a", "z"]);
    let stored_conditional = interner
        .store()
        .conditional_type(conditional)
        .expect("reserved conditional must retain its backing row");
    assert_eq!(stored_conditional.check, conditional_body.check);
    assert_eq!(stored_conditional.extends_ty, conditional_body.extends_ty);
    assert_eq!(stored_conditional.true_branch, conditional_body.true_branch);
    assert_eq!(
        stored_conditional.false_branch,
        conditional_body.false_branch
    );
    assert_eq!(stored_conditional.infer_count, conditional_body.infer_count);
    assert_eq!(
        stored_conditional.distributive,
        conditional_body.distributive
    );
    assert_eq!(stored_conditional.poisoned, conditional_body.poisoned);
    let stored_mapped = interner
        .store()
        .mapped_type(mapped)
        .expect("reserved mapped type must retain its backing row");
    assert_eq!(stored_mapped.homomorphic, mapped_body.homomorphic);
    assert_eq!(stored_mapped.key_source, mapped_body.key_source);
    assert_eq!(stored_mapped.value_template, mapped_body.value_template);
    assert_eq!(stored_mapped.modifiers_source, mapped_body.modifiers_source);
    assert_eq!(
        stored_mapped.optional_modifier,
        mapped_body.optional_modifier
    );
    assert_eq!(
        stored_mapped.readonly_modifier,
        mapped_body.readonly_modifier
    );
    assert_eq!(
        interner.fill_reserved_type_batch(vec![ReservedTypeFill::Object(
            object,
            ObjectType::default(),
        )]),
        Err(ReservedTypeFillError::AlreadyFrozen(object))
    );
    assert_eq!(
        interner.fill_reserved_type_batch(vec![ReservedTypeFill::Conditional(
            conditional,
            conditional_body,
        )]),
        Err(ReservedTypeFillError::AlreadyFrozen(conditional))
    );
    assert_eq!(
        interner.fill_reserved_type_batch(vec![ReservedTypeFill::Mapped(mapped, mapped_body)]),
        Err(ReservedTypeFillError::AlreadyFrozen(mapped))
    );
}

#[test]
fn reserved_type_batch_prevalidation_is_all_or_nothing() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let first = interner.reserve_object();
    let second = interner.reserve_object();

    assert_eq!(
        interner.fill_reserved_type_batch(vec![
            ReservedTypeFill::Object(
                first,
                ObjectType {
                    properties: vec![prop("first", wk.number)],
                    ..Default::default()
                },
            ),
            ReservedTypeFill::Object(first, ObjectType::default()),
            ReservedTypeFill::Object(
                second,
                ObjectType {
                    properties: vec![prop("second", wk.string)],
                    ..Default::default()
                },
            ),
        ]),
        Err(ReservedTypeFillError::Duplicate(first))
    );
    assert!(
        interner
            .store()
            .object_type(first)
            .expect("reserved object must remain readable")
            .properties
            .is_empty(),
        "duplicate prevalidation must precede every body write"
    );
    assert!(interner
        .store()
        .object_type(second)
        .expect("reserved object must remain readable")
        .properties
        .is_empty());

    assert_eq!(
        interner.fill_reserved_type_batch(vec![
            ReservedTypeFill::Object(
                first,
                ObjectType {
                    properties: vec![prop("first", wk.number)],
                    ..Default::default()
                },
            ),
            ReservedTypeFill::Object(
                second,
                ObjectType {
                    properties: vec![prop("second", wk.string)],
                    ..Default::default()
                },
            ),
        ]),
        Ok(())
    );
}

#[test]
fn reserved_type_batch_rejects_target_kind_and_state_before_writing() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let not_reserved = wk.number;
    let pending = interner.reserve_object();

    assert_eq!(
        interner.fill_reserved_type_batch(vec![
            ReservedTypeFill::Object(pending, ObjectType::default()),
            ReservedTypeFill::Object(not_reserved, ObjectType::default()),
        ]),
        Err(ReservedTypeFillError::NotReserved(not_reserved))
    );
    assert!(
        interner
            .store()
            .object_type(pending)
            .expect("reserved object must remain readable")
            .properties
            .is_empty(),
        "target prevalidation must precede every body write"
    );

    let conditional_body = ConditionalType {
        check: wk.number,
        extends_ty: wk.number,
        true_branch: wk.string,
        false_branch: wk.boolean,
        infer_count: 0,
        distributive: false,
        poisoned: false,
    };
    assert_eq!(
        interner.fill_reserved_type_batch(vec![ReservedTypeFill::Conditional(
            pending,
            conditional_body,
        )]),
        Err(ReservedTypeFillError::KindMismatch {
            id: pending,
            reserved: ReservedTypeKind::Object,
            supplied: ReservedTypeKind::Conditional,
        })
    );

    interner
        .fill_reserved_type_batch(vec![ReservedTypeFill::Object(
            pending,
            ObjectType {
                properties: vec![prop("stable", wk.number)],
                ..Default::default()
            },
        )])
        .expect("failed batches must leave the valid target pending");
    let other = interner.reserve_object();
    assert_eq!(
        interner.fill_reserved_type_batch(vec![
            ReservedTypeFill::Object(other, ObjectType::default()),
            ReservedTypeFill::Object(pending, ObjectType::default()),
        ]),
        Err(ReservedTypeFillError::AlreadyFrozen(pending))
    );
    assert!(
        interner
            .store()
            .object_type(other)
            .expect("reserved object must remain readable")
            .properties
            .is_empty(),
        "state prevalidation must precede every body write"
    );
    interner
        .fill_reserved_type_batch(vec![ReservedTypeFill::Object(other, ObjectType::default())])
        .expect("a frozen sibling must not partially freeze a pending target");
    assert_eq!(
        interner
            .store()
            .object_type(pending)
            .expect("filled object must remain readable")
            .properties[0]
            .key
            .as_string(),
        Some("stable")
    );
}

#[test]
fn reserved_terminalization_rejects_wrong_kind_and_double_use_without_mutation() {
    let mut interner = Interner::with_intrinsics();
    let error = interner.well_known().error;
    let conditional = interner.reserve_conditional();
    let mapped = interner.reserve_mapped();
    let object = interner.reserve_object();

    assert_eq!(
        interner.poison_reserved_conditional(mapped),
        Err(ReservedTypeFillError::KindMismatch {
            id: mapped,
            reserved: ReservedTypeKind::Mapped,
            supplied: ReservedTypeKind::Conditional,
        })
    );
    assert_eq!(
        interner.poison_reserved_mapped(object),
        Err(ReservedTypeFillError::KindMismatch {
            id: object,
            reserved: ReservedTypeKind::Object,
            supplied: ReservedTypeKind::Mapped,
        })
    );
    assert_eq!(
        interner.abandon_reserved_object(conditional),
        Err(ReservedTypeFillError::KindMismatch {
            id: conditional,
            reserved: ReservedTypeKind::Conditional,
            supplied: ReservedTypeKind::Object,
        })
    );

    interner
        .poison_reserved_conditional(conditional)
        .expect("wrong-kind attempt leaves conditional pending");
    interner
        .poison_reserved_mapped(mapped)
        .expect("wrong-kind attempt leaves mapped pending");
    interner
        .abandon_reserved_object(object)
        .expect("wrong-kind attempt leaves object pending");

    let conditional_body = interner
        .store()
        .conditional_type(conditional)
        .expect("poisoned conditional body");
    assert_eq!(conditional_body.check, error);
    assert_eq!(conditional_body.extends_ty, error);
    assert_eq!(conditional_body.true_branch, error);
    assert_eq!(conditional_body.false_branch, error);
    assert!(conditional_body.poisoned);
    let mapped_body = interner
        .store()
        .mapped_type(mapped)
        .expect("poisoned mapped body");
    assert_eq!(mapped_body.key_source, error);
    assert_eq!(mapped_body.value_template, error);
    assert!(interner
        .store()
        .object_type(object)
        .expect("abandoned object row")
        .properties
        .is_empty());

    assert_eq!(
        interner.poison_reserved_conditional(conditional),
        Err(ReservedTypeFillError::AlreadyFrozen(conditional))
    );
    assert_eq!(
        interner.poison_reserved_mapped(mapped),
        Err(ReservedTypeFillError::AlreadyFrozen(mapped))
    );
    assert_eq!(
        interner.abandon_reserved_object(object),
        Err(ReservedTypeFillError::AlreadyFrozen(object))
    );
    assert!(
        interner
            .store()
            .conditional_type(conditional)
            .expect("double terminalization preserves conditional")
            .poisoned,
        "double terminalization must not rewrite the frozen body"
    );
}

#[test]
fn caller_certified_acyclic_object_promotion_preserves_the_dedup_partition() {
    let mut interner = Interner::with_intrinsics();
    let number = interner.well_known().number;

    let unique = interner.reserve_object();
    assert_eq!(
        interner.promote_caller_certified_acyclic_reserved_object(unique),
        Err(ReservedObjectPromotionError::NotFrozen(unique))
    );
    interner.fill_object(
        unique,
        ObjectType {
            properties: vec![prop("value", number)],
            ..ObjectType::default()
        },
    );
    assert_eq!(
        interner
            .promote_caller_certified_acyclic_reserved_object(unique)
            .expect("unique acyclic reservation promotes"),
        unique
    );
    assert_eq!(
        interner.intern_object(ObjectType {
            properties: vec![prop("value", number)],
            ..ObjectType::default()
        }),
        unique,
        "the promoted row participates in ordinary object dedup"
    );

    let collision = interner.reserve_object();
    interner.fill_object(
        collision,
        ObjectType {
            properties: vec![prop("value", number)],
            ..ObjectType::default()
        },
    );
    assert_eq!(
        interner
            .promote_caller_certified_acyclic_reserved_object(collision)
            .expect("equal reservation resolves to the canonical row"),
        unique
    );

    let mapped = interner.reserve_mapped();
    assert_eq!(
        interner.promote_caller_certified_acyclic_reserved_object(mapped),
        Err(ReservedObjectPromotionError::KindMismatch(mapped))
    );
    interner
        .poison_reserved_mapped(mapped)
        .expect("unrelated reservation can still be terminalized");
    assert!(
        interner.dedup_partitions_structural_rows_exactly(),
        "promoted row and collision orphan partition exactly"
    );
}

#[test]
fn accessor_write_type_is_part_of_object_identity() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    let mut string_writer = prop("value", wk.number);
    string_writer.write_ty = Some(wk.string);
    let mut boolean_writer = prop("value", wk.number);
    boolean_writer.write_ty = Some(wk.boolean);

    let string_object = interner.intern_object(ObjectType {
        properties: vec![string_writer.clone()],
        ..Default::default()
    });
    let string_object_again = interner.intern_object(ObjectType {
        properties: vec![string_writer],
        ..Default::default()
    });
    let boolean_object = interner.intern_object(ObjectType {
        properties: vec![boolean_writer],
        ..Default::default()
    });

    assert_eq!(string_object, string_object_again);
    assert_ne!(string_object, boolean_object);
}

/// M19: index signatures are part of object identity, distinguished by
/// presence, index kind, value type, and coexistence with named members.
#[test]
fn index_signature_is_part_of_object_identity() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    // `{ [k: string]: number }` interns consistently.
    let str_idx_num_a = interner.intern_object(ObjectType {
        string_index: Some(wk.number),
        ..Default::default()
    });
    let str_idx_num_b = interner.intern_object(ObjectType {
        string_index: Some(wk.number),
        ..Default::default()
    });
    assert_eq!(
        str_idx_num_a, str_idx_num_b,
        "{{ [k: string]: number }} must intern consistently"
    );

    // Distinct from the empty object `{}` (presence of the index signature).
    let empty = interner.intern_object(ObjectType::default());
    assert_ne!(str_idx_num_a, empty, "{{ [k: string]: number }} ≠ {{}}");

    // Distinct value type: `{ [k: string]: string }`.
    let str_idx_str = interner.intern_object(ObjectType {
        string_index: Some(wk.string),
        ..Default::default()
    });
    assert_ne!(
        str_idx_num_a, str_idx_str,
        "differing index value type ⇒ distinct identity"
    );

    // Distinct index kind: `{ [i: number]: number }` (number, not string).
    let num_idx_num = interner.intern_object(ObjectType {
        number_index: Some(wk.number),
        ..Default::default()
    });
    assert_ne!(
        str_idx_num_a, num_idx_num,
        "string-index ≠ number-index of the same value type"
    );

    // A named member coexists with the index signature in the identity:
    // `{ a: number }` ≠ `{ a: number; [k: string]: number }`.
    let a_only = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number)],
        ..Default::default()
    });
    let a_and_index = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number)],
        string_index: Some(wk.number),
        ..Default::default()
    });
    assert_ne!(
        a_only, a_and_index,
        "adding an index signature changes object identity"
    );
}

/// Build a required parameter `name: ty`.
fn param(name: &str, ty: TypeId) -> crate::types::repr::ParameterType {
    crate::types::repr::ParameterType::required(name, ty)
}

/// Function hash-consing (M3): structurally identical function types share one
/// `TypeId`, while a different parameter type, return type, or arity does not.
/// Parameters are **positional**, so two functions whose parameter *types*
/// appear in a different order remain distinct.
#[test]
fn function_interning_dedups_by_signature() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    // `(x: number) => string`
    let f1 = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![param("x", wk.number)],
        ret: wk.string,
    });
    // The exact same signature interns to the same id.
    let f1_again = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![param("x", wk.number)],
        ret: wk.string,
    });
    assert_eq!(
        f1, f1_again,
        "identical function signatures must share an id"
    );

    let f_receiver = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: Some(wk.number),
        params: vec![param("x", wk.number)],
        ret: wk.string,
    });
    assert_ne!(f1, f_receiver, "receiver type is part of function identity");

    // A different return type is a distinct function type.
    let f_ret = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![param("x", wk.number)],
        ret: wk.number,
    });
    assert_ne!(f1, f_ret, "differing return types must not dedup");

    // A different parameter type is a distinct function type.
    let f_param = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![param("x", wk.string)],
        ret: wk.string,
    });
    assert_ne!(f1, f_param, "differing parameter types must not dedup");

    // Different arity is distinct.
    let f_arity = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![param("x", wk.number), param("y", wk.string)],
        ret: wk.string,
    });
    assert_ne!(f1, f_arity, "differing arity must not dedup");

    // Parameters are positional: `(a: number, b: string)` and
    // `(a: string, b: number)` are the same arity with the same *set* of
    // parameter types but in a different order — they must NOT dedup.
    let ab = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![param("a", wk.number), param("b", wk.string)],
        ret: wk.void,
    });
    let ba = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![param("a", wk.string), param("b", wk.number)],
        ret: wk.void,
    });
    assert_ne!(ab, ba, "parameter order is part of function identity");
}

#[test]
fn function_interning_includes_generic_binders() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let t = interner.intern_type_param(TypeParamId(40), "T");

    let generic = |constraint, default| FunctionType {
        type_params: vec![GenericTypeParam {
            id: TypeParamId(40),
            constraint,
            default,
        }],
        receiver: None,
        params: vec![param("value", t)],
        ret: t,
    };

    let first = interner.intern_function(generic(Some(wk.number), Some(wk.string)));
    let again = interner.intern_function(generic(Some(wk.number), Some(wk.string)));
    let different_constraint = interner.intern_function(generic(Some(wk.string), Some(wk.string)));
    let different_default = interner.intern_function(generic(Some(wk.number), Some(wk.number)));

    assert_eq!(first, again, "equal binders must deduplicate");
    assert_ne!(
        first, different_constraint,
        "constraints are identity-bearing"
    );
    assert_ne!(first, different_default, "defaults are identity-bearing");
}

/// M32/WU2: optional/default/rest parameter shape is part of function identity.
#[test]
fn function_interning_includes_signature_shape() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let string_array = interner.intern_array(wk.string);

    let required = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::required("x", wk.number)],
        ret: wk.void,
    });
    let optional = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::optional("x", wk.number)],
        ret: wk.void,
    });
    let optional_again = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::optional("x", wk.number)],
        ret: wk.void,
    });
    let defaulted = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::defaulted("x", wk.number)],
        ret: wk.void,
    });
    let rest = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![ParameterType::rest("x", string_array)],
        ret: wk.void,
    });

    assert_eq!(optional, optional_again, "identical optional shape dedups");
    assert_ne!(required, optional, "required and optional are distinct");
    assert_ne!(optional, defaulted, "optional and defaulted are distinct");
    assert_ne!(required, rest, "required and rest are distinct");

    let mixed = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: vec![
            ParameterType::required("a", wk.number),
            ParameterType::optional("b", wk.boolean),
            ParameterType::rest("args", string_array),
        ],
        ret: wk.void,
    });
    let stored = interner
        .store()
        .function_type(mixed)
        .expect("mixed is a function");
    assert_eq!(stored.required_param_count(), 1);
    assert_eq!(stored.total_fixed_param_count(), 2);
    assert!(stored.has_rest_param());
    assert_eq!(
        stored.rest_param().map(|param| param.ty),
        Some(string_array)
    );
}

/// Union canonicalization + hash-consing (mvp-plan §3.3, M4 — a
/// correctness-critical invariant). Order-independence, dedup, `never`-drop,
/// single-member collapse, top-type absorption, and flatten are all asserted
/// against the resulting `TypeId`s.
#[test]
fn union_canonicalization_and_hash_consing() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    // Order-independence: `number | string` and `string | number` are the
    // same canonical `TypeId`.
    let ns = interner.union(vec![wk.number, wk.string]);
    let sn = interner.union(vec![wk.string, wk.number]);
    assert_eq!(ns, sn, "union member order must not affect identity");
    assert_eq!(
        interner.store().tag(ns),
        TypeTag::Union,
        "a 2-member union must be a union node"
    );
    // The stored members are sorted by TypeId.
    let members = interner
        .store()
        .union_members(ns)
        .expect("ns is a union")
        .to_vec();
    let mut sorted = members.clone();
    sorted.sort_unstable();
    assert_eq!(members, sorted, "stored members must be TypeId-sorted");
    assert_eq!(members.len(), 2);

    // Dedup: `number | number` collapses to plain `number` (no union node).
    let nn = interner.union(vec![wk.number, wk.number]);
    assert_eq!(nn, wk.number, "a duplicated single member collapses");

    // `never` is dropped: `number | never` → `number`.
    let n_never = interner.union(vec![wk.number, wk.never]);
    assert_eq!(n_never, wk.number, "never must be absorbed out of a union");

    // A union of a single distinct member collapses to that member.
    let single = interner.union(vec![wk.boolean]);
    assert_eq!(
        single, wk.boolean,
        "a 1-member union collapses to the member"
    );

    // An empty union (or one of only `never`s) collapses to `never`.
    assert_eq!(interner.union(vec![]), wk.never, "empty union → never");
    assert_eq!(
        interner.union(vec![wk.never, wk.never]),
        wk.never,
        "a union of only never → never"
    );

    // Absorption: `any` swallows the whole union; `unknown` swallows when no
    // `any` is present.
    assert_eq!(
        interner.union(vec![wk.number, wk.any]),
        wk.any,
        "any absorbs the union"
    );
    assert_eq!(
        interner.union(vec![wk.number, wk.unknown]),
        wk.unknown,
        "unknown absorbs the union"
    );
    // `any` wins over `unknown` when both appear.
    assert_eq!(
        interner.union(vec![wk.unknown, wk.any]),
        wk.any,
        "any wins over unknown"
    );

    // Flatten: a nested union is expanded, then canonicalized. `(number |
    // string) | boolean` ≡ `number | string | boolean` (built directly),
    // sharing one id.
    let nsb_nested = interner.union(vec![ns, wk.boolean]);
    let nsb_flat = interner.union(vec![wk.number, wk.string, wk.boolean]);
    assert_eq!(nsb_nested, nsb_flat, "nested unions must flatten");
    assert_eq!(
        interner
            .store()
            .union_members(nsb_flat)
            .expect("nsb is a union")
            .len(),
        3,
        "flattened union has all three members"
    );

    // Re-interning the same canonical union returns the same id (hash-cons).
    let nsb_again = interner.union(vec![wk.boolean, wk.string, wk.number]);
    assert_eq!(nsb_flat, nsb_again, "identical unions hash-cons to one id");
}

/// M31 intersection canonicalization: dual absorption/identity rules, flattening,
/// dedup, collapse, and distinctness from unions over the same member set.
#[test]
fn intersection_canonicalization_and_hash_consing() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    // Two object members so a genuine ≥ 2-member node is built (disjoint primitives
    // are NOT reduced, but two objects give a clean structural node).
    let a = interner.intern_object(ObjectType {
        properties: vec![prop("a", wk.number)],
        ..Default::default()
    });
    let b = interner.intern_object(ObjectType {
        properties: vec![prop("b", wk.string)],
        ..Default::default()
    });

    // Order-independence: `A & B` and `B & A` are the same canonical `TypeId`.
    let ab = interner.intersection(vec![a, b]);
    let ba = interner.intersection(vec![b, a]);
    assert_eq!(ab, ba, "intersection member order must not affect identity");
    assert_eq!(
        interner.store().tag(ab),
        TypeTag::Intersection,
        "a 2-member intersection must be an intersection node"
    );
    // The stored members are sorted by TypeId.
    let members = interner
        .store()
        .intersection_members(ab)
        .expect("ab is an intersection")
        .to_vec();
    let mut sorted = members.clone();
    sorted.sort_unstable();
    assert_eq!(members, sorted, "stored members must be TypeId-sorted");
    assert_eq!(members.len(), 2);

    // Dedup: `A & A` collapses to plain `A` (no intersection node).
    let aa = interner.intersection(vec![a, a]);
    assert_eq!(aa, a, "a duplicated single member collapses");

    // `unknown` is dropped: `A & unknown` → `A` (unknown is the identity of `&`).
    let a_unknown = interner.intersection(vec![a, wk.unknown]);
    assert_eq!(a_unknown, a, "unknown must be dropped from an intersection");

    // `never` absorbs: `A & never` → `never` (never is the bottom of `&`).
    let a_never = interner.intersection(vec![a, wk.never]);
    assert_eq!(a_never, wk.never, "never absorbs the whole intersection");

    // A single distinct member collapses to that member.
    let single = interner.intersection(vec![a]);
    assert_eq!(single, a, "a 1-member intersection collapses to the member");

    // An empty intersection (or one of only `unknown`s) collapses to `unknown`.
    assert_eq!(
        interner.intersection(vec![]),
        wk.unknown,
        "empty intersection → unknown"
    );
    assert_eq!(
        interner.intersection(vec![wk.unknown, wk.unknown]),
        wk.unknown,
        "an intersection of only unknown → unknown"
    );

    // Absorption: `any` swallows an ordinary intersection member (cascade
    // suppression).
    assert_eq!(
        interner.intersection(vec![a, wk.any]),
        wk.any,
        "any absorbs the intersection"
    );
    // tsc checks Never before Any in `getIntersectionType`, so `never` annihilates:
    // `any & never` is `never`, not `any`.
    assert_eq!(
        interner.intersection(vec![wk.never, wk.any]),
        wk.never,
        "never annihilates (any & never = never)"
    );
    assert_eq!(
        interner.intersection(vec![wk.any, wk.never]),
        wk.never,
        "never annihilates regardless of member order"
    );
    assert_eq!(
        interner.intersection(vec![a, wk.never]),
        wk.never,
        "never absorbs an ordinary member"
    );
    // The internal error type stays absorbing even against `never` (deliberate
    // cascade suppression for the distinct error type — `error & never` = `any`).
    assert_eq!(
        interner.intersection(vec![wk.error, wk.never]),
        wk.any,
        "error keeps suppressing cascades, even with never present"
    );

    // Flatten: `(A & B) & C` ≡ `A & B & C` (built directly), sharing one id.
    let c = interner.intern_object(ObjectType {
        properties: vec![prop("c", wk.boolean)],
        ..Default::default()
    });
    let abc_nested = interner.intersection(vec![ab, c]);
    let abc_flat = interner.intersection(vec![a, b, c]);
    assert_eq!(abc_nested, abc_flat, "nested intersections must flatten");
    assert_eq!(
        interner
            .store()
            .intersection_members(abc_flat)
            .expect("abc is an intersection")
            .len(),
        3,
        "flattened intersection has all three members"
    );

    // Re-interning the same canonical intersection returns the same id (hash-cons).
    let abc_again = interner.intersection(vec![c, b, a]);
    assert_eq!(
        abc_flat, abc_again,
        "identical intersections hash-cons to one id"
    );

    // A union and an intersection over the same member set never collide (distinct
    // discriminants), even though both are 2-member sets of {A, B}.
    let union_ab = interner.union(vec![a, b]);
    assert_ne!(
        union_ab, ab,
        "a union and an intersection over the same set must be distinct types"
    );
}

#[test]
fn intersection_reduces_disjoint_primitive_domains() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let domains = [
        vec![wk.null],
        vec![wk.void, wk.undefined],
        vec![wk.boolean],
        vec![wk.number],
        vec![wk.string],
        vec![wk.bigint],
        vec![wk.symbol],
        vec![wk.object],
    ];

    for (index, domain) in domains.iter().enumerate() {
        for &left in domain {
            for &right in domain {
                assert_ne!(
                    interner.intersection(vec![left, right]),
                    wk.never,
                    "members of one domain may overlap"
                );
            }
            for other_domain in &domains[index + 1..] {
                for &right in other_domain {
                    assert_eq!(
                        interner.intersection(vec![left, right]),
                        wk.never,
                        "different primitive domains are disjoint"
                    );
                    assert_eq!(
                        interner.intersection(vec![right, left]),
                        wk.never,
                        "primitive-domain reduction is order independent"
                    );
                    assert_eq!(
                        interner.intersection(vec![left, right, left]),
                        wk.never,
                        "duplicates do not hide disjoint domains"
                    );
                }
            }
        }
    }

    let brand = interner.intern_object(ObjectType {
        properties: vec![prop("brand", wk.string)],
        ..Default::default()
    });
    let branded_string = interner.intersection(vec![wk.string, brand]);
    assert_eq!(interner.store().tag(branded_string), TypeTag::Intersection);
    assert_eq!(
        interner.intersection(vec![wk.number, branded_string]),
        wk.never,
        "flattening exposes disjoint domains in nested intersections"
    );

    assert_eq!(
        interner.intersection(vec![wk.any, wk.string, wk.number]),
        wk.any,
        "any absorption precedes primitive reduction"
    );
    assert_eq!(
        interner.intersection(vec![wk.error, wk.string, wk.number]),
        wk.any,
        "error absorption precedes primitive reduction"
    );
    assert_eq!(
        interner.intersection(vec![wk.never, wk.string, wk.number]),
        wk.never,
        "never absorption precedes primitive reduction"
    );
    assert_eq!(
        interner.intersection(vec![wk.unknown, wk.string, wk.number]),
        wk.never,
        "unknown remains the identity before primitive reduction"
    );
}

#[test]
fn intersection_reduces_only_proven_disjoint_singletons() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    let string_x = interner.intern_literal(LiteralValue::String("x".to_owned()));
    let string_x_again = interner.intern_literal(LiteralValue::String("x".to_owned()));
    let string_y = interner.intern_literal(LiteralValue::String("y".to_owned()));
    let number_one = interner.intern_literal(LiteralValue::Number(1.0));
    let number_two = interner.intern_literal(LiteralValue::Number(2.0));
    let positive_zero = interner.intern_literal(LiteralValue::Number(0.0));
    let negative_zero = interner.intern_literal(LiteralValue::Number(-0.0));
    let boolean_true = interner.intern_literal(LiteralValue::Boolean(true));
    let boolean_false = interner.intern_literal(LiteralValue::Boolean(false));

    assert_eq!(string_x, string_x_again);
    assert_eq!(
        interner.intersection(vec![string_x, string_x_again]),
        string_x,
        "equal singleton literals remain inhabited"
    );
    assert_ne!(
        interner.intersection(vec![positive_zero, negative_zero]),
        wk.never,
        "positive and negative zero denote the same singleton value"
    );

    for (left, right) in [
        (string_x, string_y),
        (number_one, number_two),
        (boolean_true, boolean_false),
    ] {
        assert_eq!(
            interner.intersection(vec![left, right]),
            wk.never,
            "unequal singleton literals in one domain are disjoint"
        );
        assert_eq!(
            interner.intersection(vec![right, left]),
            wk.never,
            "singleton reduction is order independent"
        );
    }

    for (primitive, literal) in [
        (wk.string, string_x),
        (wk.number, number_one),
        (wk.boolean, boolean_true),
    ] {
        let overlap = interner.intersection(vec![primitive, literal]);
        assert_eq!(
            interner.store().tag(overlap),
            TypeTag::Intersection,
            "a primitive and its own literal subtype overlap"
        );
    }

    assert_eq!(
        interner.intersection(vec![string_x, number_one]),
        wk.never,
        "singleton literals from different domains are disjoint"
    );
}

#[test]
fn intersection_preserves_nonstructural_disjointness_boundaries() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    let brand = interner.intern_object(ObjectType {
        properties: vec![prop("brand", wk.string)],
        ..Default::default()
    });
    let parameter = interner.intern_type_param(TypeParamId(107), "T");
    let template = interner.intern_template(TemplateType {
        texts: vec!["prefix-".to_owned(), String::new()],
        holes: vec![parameter],
    });

    for opaque in [brand, parameter, template] {
        let intersection = interner.intersection(vec![wk.string, opaque]);
        assert_eq!(
            interner.store().tag(intersection),
            TypeTag::Intersection,
            "non-primitive boundaries remain potentially inhabited"
        );
        assert_ne!(intersection, wk.never);
    }
}

#[test]
fn intersection_proves_disjoint_finite_union_domains() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    let string_or_number = interner.union(vec![wk.string, wk.number]);
    let boolean_or_symbol = interner.union(vec![wk.boolean, wk.symbol]);
    assert_eq!(
        interner.intersection(vec![string_or_number, wk.boolean]),
        wk.never,
        "every union branch is disjoint from boolean"
    );
    assert_eq!(
        interner.intersection(vec![wk.boolean, string_or_number]),
        wk.never,
        "union disjointness is order independent"
    );
    assert_eq!(
        interner.intersection(vec![string_or_number, boolean_or_symbol]),
        wk.never,
        "recursive union comparison proves every branch pairing"
    );
    assert_eq!(
        interner.intersection(vec![string_or_number, wk.object]),
        wk.never,
        "primitive union branches are disjoint from object"
    );

    let string_x = interner.intern_literal(LiteralValue::String("x".to_owned()));
    let string_y = interner.intern_literal(LiteralValue::String("y".to_owned()));
    let string_z = interner.intern_literal(LiteralValue::String("z".to_owned()));
    let x_or_y = interner.union(vec![string_x, string_y]);
    assert_eq!(
        interner.intersection(vec![x_or_y, string_z]),
        wk.never,
        "every finite literal branch is disjoint from z"
    );

    for overlap in [
        interner.intersection(vec![string_or_number, wk.string]),
        interner.intersection(vec![x_or_y, string_x]),
    ] {
        assert_eq!(
            interner.store().tag(overlap),
            TypeTag::Intersection,
            "one overlapping union branch prevents a disjointness proof"
        );
    }

    let object_or_string = interner.union(vec![wk.object, wk.string]);
    let object_overlap = interner.intersection(vec![object_or_string, wk.object]);
    assert_eq!(
        interner.store().tag(object_overlap),
        TypeTag::Intersection,
        "the object branch keeps the union intersection potentially inhabited"
    );

    let brand = interner.intern_object(ObjectType {
        properties: vec![prop("brand", wk.string)],
        ..Default::default()
    });
    let branded_union = interner.union(vec![brand, wk.number]);
    let branded_overlap = interner.intersection(vec![branded_union, wk.string]);
    assert_eq!(
        interner.store().tag(branded_overlap),
        TypeTag::Intersection,
        "an opaque structural-object branch blocks the narrow proof"
    );
}

/// Array hash-consing (M17): an array's identity is its element id alone, so
/// `number[]` interns consistently, `number[]` ≠ `string[]`, and `number[][]`
/// nests (its element is `number[]`). The element is canonical, so the dedup
/// tie-break is an id compare.
#[test]
fn array_interning_dedups_by_element() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    // `number[]` interns to one shared id regardless of how many times built.
    let num_arr_a = interner.intern_array(wk.number);
    let num_arr_b = interner.intern_array(wk.number);
    assert_eq!(num_arr_a, num_arr_b, "number[] must intern consistently");
    assert_eq!(
        interner.store().tag(num_arr_a),
        TypeTag::Array,
        "an array is an Array-tagged row"
    );
    assert_eq!(
        interner.store().array_type(num_arr_a).map(|a| a.element),
        Some(wk.number),
        "the stored element is `number`"
    );

    // A different element is a distinct array type.
    let str_arr = interner.intern_array(wk.string);
    assert_ne!(num_arr_a, str_arr, "number[] ≠ string[]");

    // Nesting: `number[][]` has `number[]` as its element and is distinct from
    // `number[]`.
    let num_arr_arr = interner.intern_array(num_arr_a);
    assert_ne!(num_arr_arr, num_arr_a, "number[][] ≠ number[]");
    assert_eq!(
        interner.store().array_type(num_arr_arr).map(|a| a.element),
        Some(num_arr_a),
        "number[][]'s element is number[]"
    );

    // An array of a union element interns by that (canonical) union id —
    // member order in the union does not change the array's identity.
    let union_ns = interner.union(vec![wk.number, wk.string]);
    let union_sn = interner.union(vec![wk.string, wk.number]);
    let union_arr_a = interner.intern_array(union_ns);
    let union_arr_b = interner.intern_array(union_sn);
    assert_eq!(
        union_arr_a, union_arr_b,
        "(number | string)[] interns consistently via the canonical union element"
    );
}

/// Tuple hash-consing (M18): a tuple's identity is its **ordered** element
/// list, so `[number, string]` interns consistently while `[number, string]`,
/// `[string, number]` (order differs), and `[number]` (arity differs) are all
/// distinct. Order is significant — the list is never sorted (unlike a union).
#[test]
fn tuple_interning_is_order_significant() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    // `[number, string]` interns to one shared id regardless of how many times
    // built.
    let ns_a = interner.intern_tuple(vec![wk.number, wk.string]);
    let ns_b = interner.intern_tuple(vec![wk.number, wk.string]);
    assert_eq!(ns_a, ns_b, "[number, string] must intern consistently");
    assert_eq!(
        interner.store().tag(ns_a),
        TypeTag::Tuple,
        "a tuple is a Tuple-tagged row"
    );

    // Order matters: `[string, number]` is a DISTINCT tuple (NOT sorted into the
    // same canonical form a union would be).
    let sn = interner.intern_tuple(vec![wk.string, wk.number]);
    assert_ne!(
        ns_a, sn,
        "[number, string] ≠ [string, number] (order-significant)"
    );

    // Arity matters: `[number]` is distinct from `[number, string]`.
    let single = interner.intern_tuple(vec![wk.number]);
    assert_ne!(ns_a, single, "[number, string] ≠ [number] (arity differs)");
    assert_ne!(sn, single, "[string, number] ≠ [number]");

    // The stored element list preserves source order exactly.
    let stored = interner
        .store()
        .tuple_type(ns_a)
        .expect("ns_a is a tuple")
        .elements
        .clone();
    assert_eq!(
        stored,
        vec![wk.number, wk.string],
        "stored order is source order"
    );

    // The empty tuple `[]` is a valid distinct tuple (and interns consistently).
    let empty_a = interner.intern_tuple(vec![]);
    let empty_b = interner.intern_tuple(vec![]);
    assert_eq!(empty_a, empty_b, "[] interns consistently");
    assert_ne!(empty_a, single, "[] ≠ [number]");

    // A tuple is distinct from the array of the same element (different tags).
    let num_arr = interner.intern_array(wk.number);
    let num_tuple = interner.intern_tuple(vec![wk.number]);
    assert_ne!(num_arr, num_tuple, "number[] ≠ [number]");

    // Nesting: a tuple element may itself be a tuple, interned by the inner id.
    let nested_a = interner.intern_tuple(vec![ns_a, wk.boolean]);
    let nested_b = interner.intern_tuple(vec![ns_b, wk.boolean]); // ns_b == ns_a
    assert_eq!(
        nested_a, nested_b,
        "nested tuple identity propagates by element id"
    );
}

/// M32/WU2: tuple rest position and rest type are part of tuple identity.
#[test]
fn tuple_interning_includes_rest_shape() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let string_array = interner.intern_array(wk.string);
    let number_array = interner.intern_array(wk.number);

    let fixed_number = interner.intern_tuple(vec![wk.number]);
    let trailing = interner.intern_tuple_type(TupleType::with_rest(
        vec![wk.number],
        TupleRestType::new(1, string_array),
    ));
    let trailing_again = interner.intern_tuple_type(TupleType::with_rest(
        vec![wk.number],
        TupleRestType::new(1, string_array),
    ));
    let leading = interner.intern_tuple_type(TupleType::with_rest(
        vec![wk.number],
        TupleRestType::new(0, string_array),
    ));
    let different_rest_type = interner.intern_tuple_type(TupleType::with_rest(
        vec![wk.number],
        TupleRestType::new(1, number_array),
    ));

    assert_eq!(trailing, trailing_again, "identical rest tuple dedups");
    assert_ne!(fixed_number, trailing, "fixed tuple and rest tuple differ");
    assert_ne!(trailing, leading, "rest position is identity-bearing");
    assert_ne!(
        trailing, different_rest_type,
        "rest type is identity-bearing"
    );

    let leading_with_tail = interner.intern_tuple_type(TupleType::with_rest(
        vec![wk.boolean],
        TupleRestType::new(0, string_array),
    ));
    let stored = interner
        .store()
        .tuple_type(leading_with_tail)
        .expect("leading_with_tail is a tuple");
    assert_eq!(stored.fixed_len(), 1);
    assert!(stored.has_rest());
    assert_eq!(
        stored.rest,
        Some(TupleRestType::new(0, string_array)),
        "[...T, X] stores rest before the fixed tail"
    );
}

/// Template hash-consing (M27): a template's identity is its ordered text segments +
/// hole ids, so `` `a-${string}` `` interns consistently, differs from
/// `` `b-${string}` `` (text) and `` `a-${number}` `` (hole), and adjacent-hole
/// templates (empty interior text) are representable and distinct.
#[test]
fn template_interning_is_structural() {
    use crate::types::repr::TemplateType;
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();

    let mk = |interner: &mut Interner, texts: &[&str], holes: Vec<TypeId>| {
        interner.intern_template(TemplateType {
            texts: texts.iter().map(|s| s.to_string()).collect(),
            holes,
        })
    };

    let a = mk(&mut interner, &["a-", ""], vec![wk.string]);
    let a2 = mk(&mut interner, &["a-", ""], vec![wk.string]);
    assert_eq!(a, a2, "identical templates hash-cons to one id");
    assert_eq!(interner.store().tag(a), TypeTag::Template);

    let b = mk(&mut interner, &["b-", ""], vec![wk.string]);
    assert_ne!(a, b, "differing text ⇒ distinct template");

    let num = mk(&mut interner, &["a-", ""], vec![wk.number]);
    assert_ne!(a, num, "differing hole type ⇒ distinct template");

    // Adjacent holes (empty interior text) are representable and distinct from a
    // separated form.
    let adjacent = mk(&mut interner, &["", "", ""], vec![wk.string, wk.string]);
    let separated = mk(&mut interner, &["", "-", ""], vec![wk.string, wk.string]);
    assert_ne!(adjacent, separated, "adjacency is part of identity");
}

/// The well-known intrinsic ids are assigned in `IntrinsicKind::ALL` order
/// and are stable/small — the property the relation engine relies on.
#[test]
fn intrinsics_get_small_fixed_ids() {
    let interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    // Error is interned first (id 0), per ALL order.
    assert_eq!(wk.error, TypeId(0));
    // All ten intrinsics are distinct and within the first ten ids.
    let ids = [
        wk.error,
        wk.any,
        wk.unknown,
        wk.never,
        wk.void,
        wk.null,
        wk.undefined,
        wk.boolean,
        wk.number,
        wk.string,
    ];
    for (i, id) in ids.iter().enumerate() {
        assert!(id.0 < IntrinsicKind::ALL.len() as u32);
        // No duplicates among the well-known ids.
        assert_eq!(ids.iter().filter(|x| **x == *id).count(), 1, "dup at {i}");
    }
    assert_eq!(interner.store().len(), IntrinsicKind::ALL.len());
}

#[test]
fn freeze_does_not_publish_transient_occurrence_derivations() {
    let mut interner = Interner::with_intrinsics();
    let string = interner.well_known().string;
    let application = interner.intern_class_instance(ClassId(80_000), vec![string]);
    let occurrence = interner
        .class_instance_occurrence_derived(application)
        .expect("class application has an occurrence graph");
    assert!(occurrence.derivation.is_some());
    let (base_before, local_before) = interner.derivation_storage_counts_for_test();
    assert_eq!(base_before, 0);
    assert!(local_before > 0, "negative control must allocate provenance");

    interner.freeze_as_base().expect("complete interner seals");
    assert_eq!(
        interner.derivation_storage_counts_for_test(),
        (0, 0),
        "freezing semantic types must discard transient occurrence provenance"
    );
    let delta = interner.fork_delta().expect("sealed interner forks");
    assert_eq!(delta.derivation_storage_counts_for_test(), (0, 0));
}

use super::*;
use crate::diagnostics::render_type;
use crate::types::repr::{
    ClassId, ConditionalType, FunctionType, GenericTypeParam, LiteralValue, MappedType, ModifierOp,
    ObjectType, ParameterType, PropertyType, TupleRestType, TupleType, TypeParamId, TypeTag,
};

/// Build a required public property `name: ty`.
fn prop(name: &str, ty: TypeId) -> PropertyType {
    PropertyType::public(name, ty)
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
    assert_eq!(
        render_type(interner.store(), a, false),
        "class#1<number, string>"
    );
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
    assert_eq!(render_type(interner.store(), a, false), "number[string]");
}

#[test]
fn deferred_indexed_access_parenthesizes_low_precedence_objects() {
    let mut interner = Interner::with_intrinsics();
    let wk = interner.well_known();
    let key = interner.intern_literal(LiteralValue::String("value".into()));
    let union = interner.union(vec![wk.number, wk.string]);
    let intersection = interner.intersection(vec![wk.number, wk.string]);
    let function = interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver: None,
        params: Vec::new(),
        ret: wk.number,
    });
    let conditional = interner.intern_conditional(crate::types::repr::ConditionalType {
        check: wk.number,
        extends_ty: wk.number,
        true_branch: wk.string,
        false_branch: wk.boolean,
        infer_count: 0,
        distributive: false,
        poisoned: false,
    });

    for (object, expected) in [
        (union, "(number | string)[\"value\"]"),
        (intersection, "(number & string)[\"value\"]"),
        (function, "(() => number)[\"value\"]"),
        (
            conditional,
            "(number extends number ? string : boolean)[\"value\"]",
        ),
    ] {
        let access = interner.intern_deferred_indexed_access(object, key);
        assert_eq!(render_type(interner.store(), access, false), expected);
    }
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

    // The canonical stored order is name-sorted regardless of input order.
    let stored = interner
        .store()
        .object_type(ba)
        .expect("ba is an object type");
    let names: Vec<&str> = stored.properties.iter().map(|p| p.name.as_str()).collect();
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
        .map(|property| property.name.as_str())
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
            .name,
        "stable"
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

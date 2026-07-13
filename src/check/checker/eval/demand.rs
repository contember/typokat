//! Dormant one-layer evaluation seams for ADR-0006.

use super::legacy_guard::evaluation_publication_exhaustion;
use crate::check::checker::indexed_access::resolve_indexed_access;
use crate::class_semantics::{DemandOutcome, PublishedClasses};
use crate::types::repr::TypeTag;
use crate::types::store::{Store, TypeId};
use crate::types::Interner;

/// Demand one deferred indexed-access layer without evaluating its selected result.
pub(in crate::check) fn demand_one_outer_layer(
    published: &PublishedClasses,
    interner: &mut Interner,
    ty: TypeId,
) -> DemandOutcome<TypeId> {
    if let Some(reason) = evaluation_publication_exhaustion(interner.store(), &[ty], published) {
        return DemandOutcome::Exhausted(reason);
    }
    let Some(access) = interner.store().deferred_indexed_access_type(ty).copied() else {
        return DemandOutcome::Ready(ty);
    };
    if object_requires_demand(interner.store(), access.object)
        || index_requires_demand(interner.store(), access.index)
    {
        return DemandOutcome::Ready(ty);
    }
    DemandOutcome::Ready(resolve_indexed_access(
        interner,
        access.object,
        access.index,
    ))
}

fn object_requires_demand(store: &Store, object: TypeId) -> bool {
    match store.tag(object) {
        TypeTag::Readonly => store
            .readonly_operand(object)
            .is_some_and(|operand| object_requires_demand(store, operand)),
        TypeTag::Conditional
        | TypeTag::Instantiation
        | TypeTag::ClassInstance
        | TypeTag::Mapped
        | TypeTag::Template
        | TypeTag::Keyof
        | TypeTag::DeferredIndexedAccess
        | TypeTag::TypeParam
        | TypeTag::Infer
        | TypeTag::MappedValue => true,
        _ => false,
    }
}

fn index_requires_demand(store: &Store, index: TypeId) -> bool {
    match store.tag(index) {
        TypeTag::Union => store.union_members(index).is_some_and(|members| {
            members
                .iter()
                .any(|member| index_requires_demand(store, *member))
        }),
        TypeTag::TypeParam => store
            .type_param(index)
            .and_then(|parameter| store.type_param_constraint(parameter.id))
            .is_none_or(|constraint| {
                constraint == index || index_requires_demand(store, constraint)
            }),
        TypeTag::Conditional
        | TypeTag::Instantiation
        | TypeTag::ClassInstance
        | TypeTag::Mapped
        | TypeTag::Template
        | TypeTag::Keyof
        | TypeTag::DeferredIndexedAccess
        | TypeTag::Infer
        | TypeTag::MappedValue => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class_semantics::{ClassConstructionState, Exhaustion};
    use crate::types::repr::{
        ClassId, ConditionalType, LiteralValue, ObjectType, PropertyType, TypeParamId,
    };

    #[test]
    fn dormant_demand_matches_the_eager_indexed_access_kernel_matrix() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let a = interner.intern_literal(LiteralValue::String("a".into()));
        let b = interner.intern_literal(LiteralValue::String("b".into()));
        let missing = interner.intern_literal(LiteralValue::String("missing".into()));
        let zero = interner.intern_literal(LiteralValue::Number(0.0));
        let one = interner.intern_literal(LiteralValue::Number(1.0));
        let two = interner.intern_literal(LiteralValue::Number(2.0));
        let fractional = interner.intern_literal(LiteralValue::Number(0.5));
        let object = interner.intern_object(ObjectType {
            properties: vec![
                PropertyType::public("a", wk.number),
                PropertyType::public("b", wk.string),
            ],
            string_index: Some(wk.boolean),
            number_index: Some(wk.string),
            ..Default::default()
        });
        let index_only = interner.intern_object(ObjectType {
            string_index: Some(wk.boolean),
            number_index: Some(wk.string),
            ..Default::default()
        });
        let array = interner.intern_array(wk.boolean);
        let tuple = interner.intern_tuple(vec![wk.number, wk.string]);
        let readonly_tuple = interner.intern_readonly(tuple);
        let union_key = interner.union(vec![a, b]);
        let key_parameter = TypeParamId(70_001);
        let constrained_key = interner.intern_type_param(key_parameter, "K");
        interner.set_type_param_constraint(key_parameter, a);
        let published = PublishedClasses::empty();

        let cases = [
            ("property", object, a, wk.number),
            ("string index", index_only, missing, wk.boolean),
            ("number index", object, zero, wk.string),
            ("array numeric literal", array, one, wk.boolean),
            ("array bare number", array, wk.number, wk.boolean),
            ("tuple element", tuple, one, wk.string),
            ("readonly tuple", readonly_tuple, zero, wk.number),
            ("tuple out of range", tuple, two, wk.error),
            ("tuple fractional", tuple, fractional, wk.error),
            ("object bare number", object, wk.number, wk.string),
            ("constrained key", object, constrained_key, wk.number),
            ("missing property", tuple, missing, wk.error),
            ("error object", wk.error, a, wk.error),
            ("any object", wk.any, a, wk.error),
            ("error key", object, wk.error, wk.error),
            ("any key", object, wk.any, wk.error),
        ];

        for (name, object, index, expected) in cases {
            let eager = resolve_indexed_access(&mut interner, object, index);
            let deferred = interner.intern_deferred_indexed_access(object, index);
            assert_eq!(eager, expected, "eager kernel: {name}");
            assert_eq!(
                demand_one_outer_layer(&published, &mut interner, deferred),
                DemandOutcome::Ready(expected),
                "dormant demand: {name}"
            );
        }

        let eager_union = resolve_indexed_access(&mut interner, object, union_key);
        let expected_union = interner.union(vec![wk.number, wk.string]);
        let deferred_union = interner.intern_deferred_indexed_access(object, union_key);
        assert_eq!(eager_union, expected_union);
        assert_eq!(
            demand_one_outer_layer(&published, &mut interner, deferred_union),
            DemandOutcome::Ready(expected_union)
        );
    }

    #[test]
    fn one_layer_demand_preserves_deferred_operands_and_selected_results() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let key = interner.intern_literal(LiteralValue::String("value".into()));
        let nested = interner.intern_deferred_indexed_access(wk.number, wk.string);
        let object = interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("value", nested)],
            ..Default::default()
        });
        let outer = interner.intern_deferred_indexed_access(object, key);
        let conditional = interner.intern_conditional(ConditionalType {
            check: wk.number,
            extends_ty: wk.number,
            true_branch: key,
            false_branch: wk.string,
            infer_count: 0,
            distributive: false,
            poisoned: false,
        });
        let deferred_operand = interner.intern_deferred_indexed_access(object, conditional);
        let open_key = interner.intern_type_param(TypeParamId(70_002), "K");
        let open_operand = interner.intern_deferred_indexed_access(object, open_key);
        let published = PublishedClasses::empty();

        assert_eq!(
            demand_one_outer_layer(&published, &mut interner, outer),
            DemandOutcome::Ready(nested),
            "the selected nested result is not recursively evaluated"
        );
        for deferred in [deferred_operand, open_operand] {
            assert_eq!(
                demand_one_outer_layer(&published, &mut interner, deferred),
                DemandOutcome::Ready(deferred),
                "an unresolved operand keeps the exact deferred node"
            );
        }
    }

    #[test]
    fn one_layer_demand_preserves_published_class_frontiers_and_poison() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let class = ClassId(70_003);
        let application = interner.intern_class_instance(class, vec![wk.number]);
        let deferred = interner.intern_deferred_indexed_access(application, wk.string);
        let len = interner.store().len();

        let published = PublishedClasses::forged(class, ClassConstructionState::Published);
        assert_eq!(
            demand_one_outer_layer(&published, &mut interner, deferred),
            DemandOutcome::Ready(deferred)
        );
        let poisoned = PublishedClasses::forged(class, ClassConstructionState::Poisoned);
        assert_eq!(
            demand_one_outer_layer(&poisoned, &mut interner, deferred),
            DemandOutcome::Exhausted(Exhaustion::ClassHeritagePoison { class })
        );
        assert_eq!(interner.store().len(), len);
    }

    #[test]
    fn nested_class_arguments_preserve_prepublication_exhaustion() {
        let mut interner = Interner::with_intrinsics();
        let outer_class = ClassId(70_004);
        let inner_class = ClassId(70_005);
        let inner = interner.intern_class_instance(inner_class, Vec::new());
        let outer = interner.intern_class_instance(outer_class, vec![inner]);
        let published_outer =
            PublishedClasses::forged(outer_class, ClassConstructionState::Published);

        assert_eq!(
            demand_one_outer_layer(&published_outer, &mut interner, outer),
            DemandOutcome::Exhausted(Exhaustion::ClassNotPublished {
                class: inner_class,
                state: ClassConstructionState::Pending,
            })
        );
    }

    #[test]
    fn publication_preflight_visits_both_deferred_indexed_children() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let class = ClassId(70_006);
        let application = interner.intern_class_instance(class, Vec::new());
        let object_child = interner.intern_deferred_indexed_access(application, wk.string);
        let index_child = interner.intern_deferred_indexed_access(wk.number, application);
        let published = PublishedClasses::empty();
        let len = interner.store().len();

        for deferred in [object_child, index_child] {
            assert_eq!(
                demand_one_outer_layer(&published, &mut interner, deferred),
                DemandOutcome::Exhausted(Exhaustion::ClassNotPublished {
                    class,
                    state: ClassConstructionState::Pending,
                })
            );
            assert_eq!(interner.store().len(), len);
        }
    }
}

//! Evaluator-owned dormant semantic boundary and policy-specific identity walk.

use crate::class_semantics::{is_prepublication, DemandOutcome, Exhaustion, PublishedClasses};
use crate::types::repr::TypeTag;
use crate::types::store::{Store, TypeId};
use rustc_hash::FxHashSet;

pub(super) fn reject_legacy_semantic_types(store: &Store, roots: &[TypeId]) {
    if !store.has_dormant_semantic_types() {
        return;
    }
    record_root_walk();
    if find_dormant_semantic_type(store, roots).is_none() {
        return;
    }
    record_evaluation_tripwire();
    panic!("ADR-0006 dormant type reached a legacy evaluator before WU1c");
}

pub(super) fn reject_legacy_semantic_type(store: &Store, ty: TypeId) {
    if matches!(
        store.tag(ty),
        TypeTag::ClassInstance | TypeTag::DeferredIndexedAccess
    ) {
        record_evaluation_tripwire();
        panic!("ADR-0006 dormant type reached a legacy evaluator frame before WU1c");
    }
}

pub(super) fn evaluation_publication_exhaustion(
    store: &Store,
    roots: &[TypeId],
    published: &PublishedClasses,
) -> Option<Exhaustion> {
    let mut stack = roots.to_vec();
    let mut seen = FxHashSet::default();
    while let Some(ty) = stack.pop() {
        if !seen.insert(ty) {
            continue;
        }
        if let Some(instance) = store.class_instance_type(ty) {
            match published.require(instance.class) {
                DemandOutcome::Ready(()) => {}
                DemandOutcome::Exhausted(reason) => {
                    if is_prepublication(&reason) {
                        record_evaluation_tripwire();
                    }
                    return Some(reason);
                }
            }
        }
        push_evaluation_children(store, ty, &mut stack);
    }
    None
}

fn find_dormant_semantic_type(store: &Store, roots: &[TypeId]) -> Option<TypeId> {
    let mut stack = roots.to_vec();
    let mut seen = FxHashSet::default();
    while let Some(ty) = stack.pop() {
        if !seen.insert(ty) {
            continue;
        }
        if matches!(
            store.tag(ty),
            TypeTag::ClassInstance | TypeTag::DeferredIndexedAccess
        ) {
            return Some(ty);
        }
        push_evaluation_children(store, ty, &mut stack);
    }
    None
}

/// Identity children that the legacy evaluator can inspect directly or indirectly.
fn push_evaluation_children(store: &Store, ty: TypeId, stack: &mut Vec<TypeId>) {
    match store.tag(ty) {
        TypeTag::Object => {
            if let Some(object) = store.object_type(ty) {
                stack.extend(object.properties.iter().map(|property| property.ty));
                stack.extend(object.string_index);
                stack.extend(object.number_index);
                stack.extend(object.call_signatures.iter().copied());
                stack.extend(object.construct_signatures.iter().copied());
            }
        }
        TypeTag::Function => {
            if let Some(function) = store.function_type(ty) {
                stack.extend(
                    function
                        .type_params
                        .iter()
                        .flat_map(|parameter| [parameter.constraint, parameter.default])
                        .flatten(),
                );
                stack.extend(function.receiver);
                stack.extend(function.params.iter().map(|parameter| parameter.ty));
                stack.push(function.ret);
            }
        }
        TypeTag::TypeParam => {
            if let Some(parameter) = store.type_param(ty) {
                stack.extend(store.type_param_constraint(parameter.id));
            }
        }
        TypeTag::Union => stack.extend(store.union_members(ty).into_iter().flatten().copied()),
        TypeTag::Intersection => {
            stack.extend(
                store
                    .intersection_members(ty)
                    .into_iter()
                    .flatten()
                    .copied(),
            );
        }
        TypeTag::Array => {
            if let Some(array) = store.array_type(ty) {
                stack.push(array.element);
            }
        }
        TypeTag::Tuple => {
            if let Some(tuple) = store.tuple_type(ty) {
                stack.extend(tuple.elements.iter().copied());
                stack.extend(tuple.rest.map(|rest| rest.ty));
            }
        }
        TypeTag::Readonly => stack.extend(store.readonly_operand(ty)),
        TypeTag::Conditional => {
            if let Some(conditional) = store.conditional_type(ty) {
                stack.extend([
                    conditional.check,
                    conditional.extends_ty,
                    conditional.true_branch,
                    conditional.false_branch,
                ]);
            }
        }
        TypeTag::Instantiation => {
            if let Some(instantiation) = store.instantiation_type(ty) {
                stack.push(instantiation.base);
                stack.extend(instantiation.args.iter().map(|(_, argument)| *argument));
            }
        }
        TypeTag::ClassInstance => {
            if let Some(instance) = store.class_instance_type(ty) {
                stack.extend(instance.args.iter().copied());
            }
        }
        TypeTag::Mapped => {
            if let Some(mapped) = store.mapped_type(ty) {
                stack.push(mapped.key_source);
                stack.push(mapped.value_template);
                stack.extend(mapped.modifiers_source);
            }
        }
        TypeTag::Template => {
            if let Some(template) = store.template_type(ty) {
                stack.extend(template.holes.iter().copied());
            }
        }
        TypeTag::Keyof => stack.extend(store.keyof_operand(ty)),
        TypeTag::DeferredIndexedAccess => {
            if let Some(access) = store.deferred_indexed_access_type(ty) {
                stack.push(access.object);
                stack.push(access.index);
            }
        }
        TypeTag::Intrinsic | TypeTag::Literal | TypeTag::Infer | TypeTag::MappedValue => {}
    }
}

#[cfg(test)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::check::checker) struct EvaluationGuardMeasure {
    pub tripwires: u64,
    pub root_walks: u64,
}

#[cfg(test)]
thread_local! {
    static EVALUATION_GUARD_MEASURE: std::cell::RefCell<EvaluationGuardMeasure> =
        std::cell::RefCell::new(EvaluationGuardMeasure::default());
}

#[cfg(test)]
pub(in crate::check::checker) fn reset_evaluation_guard_measure() {
    EVALUATION_GUARD_MEASURE
        .with(|measure| *measure.borrow_mut() = EvaluationGuardMeasure::default());
}

#[cfg(test)]
pub(in crate::check::checker) fn evaluation_guard_measure() -> EvaluationGuardMeasure {
    EVALUATION_GUARD_MEASURE.with(|measure| *measure.borrow())
}

fn record_evaluation_tripwire() {
    #[cfg(test)]
    EVALUATION_GUARD_MEASURE.with(|measure| measure.borrow_mut().tripwires += 1);
}

fn record_root_walk() {
    #[cfg(test)]
    EVALUATION_GUARD_MEASURE.with(|measure| measure.borrow_mut().root_walks += 1);
}

//! Single checker-owned indexed-access kernel shared by eager and dormant demand.

use crate::types::repr::{LiteralValue, TypeTag};
use crate::types::store::TypeId;
use crate::types::Interner;

/// Resolve one indexed-access operation without evaluating the selected result.
pub(super) fn resolve_indexed_access(
    interner: &mut Interner,
    object: TypeId,
    index: TypeId,
) -> TypeId {
    let wk = interner.well_known();
    if object == wk.error || object == wk.any || index == wk.error || index == wk.any {
        return wk.error;
    }
    let object = interner.store().readonly_operand(object).unwrap_or(object);

    if let Some(members) = interner.store().union_members(index) {
        let members = members.to_vec();
        let resolved = members
            .into_iter()
            .map(|member| resolve_indexed_access(interner, object, member))
            .collect();
        return interner.union(resolved);
    }

    resolve_single(interner, object, index)
}

fn resolve_single(interner: &mut Interner, object: TypeId, index: TypeId) -> TypeId {
    let wk = interner.well_known();
    let store = interner.store();

    if let Some(LiteralValue::String(name)) = store.literal_value(index) {
        return store
            .object_type(object)
            .and_then(|object| {
                object
                    .property(name)
                    .map(|property| property.ty)
                    .or(object.string_index)
            })
            .unwrap_or(wk.error);
    }

    if let Some(LiteralValue::Number(number)) = store.literal_value(index) {
        if store.tag(object) == TypeTag::Tuple {
            return whole_index(*number)
                .and_then(|position| {
                    store
                        .tuple_type(object)
                        .and_then(|tuple| tuple.elements.get(position))
                        .copied()
                })
                .unwrap_or(wk.error);
        }
        if let Some(array) = store.array_type(object) {
            return array.element;
        }
        return store
            .object_type(object)
            .and_then(|object| object.number_index)
            .unwrap_or(wk.error);
    }

    if index == wk.number {
        if let Some(array) = store.array_type(object) {
            return array.element;
        }
        return store
            .object_type(object)
            .and_then(|object| object.number_index)
            .unwrap_or(wk.error);
    }

    if let Some(parameter) = store.type_param(index) {
        if let Some(constraint) = store.type_param_constraint(parameter.id) {
            if constraint != index {
                return resolve_indexed_access(interner, object, constraint);
            }
        }
    }

    wk.error
}

/// Convert a numeric literal to a tuple position.
fn whole_index(value: f64) -> Option<usize> {
    if !value.is_finite() || value.fract() != 0.0 || value < 0.0 {
        return None;
    }
    if value == 0.0 {
        return Some(0);
    }
    value.to_string().parse().ok()
}

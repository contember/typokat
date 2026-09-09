//! Pure positional projection of fixed and represented rest signature parameters.

use crate::types::repr::{ParameterType, TupleType};
use crate::types::store::{Store, TypeId};
use crate::types::Interner;

#[cfg(test)]
#[path = "tests/signature_projection.rs"]
mod tests;

#[derive(Clone, Copy)]
pub(super) struct CallArgumentTarget {
    pub declared: TypeId,
    pub accepts_undefined: bool,
}

#[derive(Clone, Copy)]
pub(super) struct RestArity {
    pub min: usize,
    pub max: Option<usize>,
}

#[derive(Clone)]
pub(super) struct RestCallShape {
    pub prefix: Vec<TypeId>,
    pub variadic: Option<TypeId>,
    pub suffix: Vec<TypeId>,
}

impl RestCallShape {
    pub fn min_len(&self) -> usize {
        self.prefix.len() + self.suffix.len()
    }

    pub fn max_len(&self) -> Option<usize> {
        if self.variadic.is_some() {
            None
        } else {
            Some(self.min_len())
        }
    }

    pub fn accepts_len(&self, len: usize) -> bool {
        if len < self.min_len() {
            return false;
        }
        self.variadic.is_some() || len == self.min_len()
    }

    pub fn element_at(&self, index: usize, len: usize) -> Option<TypeId> {
        if !self.accepts_len(len) || index >= len {
            return None;
        }
        if index < self.prefix.len() {
            return self.prefix.get(index).copied();
        }
        let suffix_start = len.saturating_sub(self.suffix.len());
        if index >= suffix_start {
            return self.suffix.get(index - suffix_start).copied();
        }
        self.variadic
    }
}

pub(super) fn rest_parameter_arity(store: &Store, rest_ty: TypeId) -> RestArity {
    if let Some(shapes) = rest_call_shapes(store, rest_ty) {
        let min = shapes.iter().map(RestCallShape::min_len).min().unwrap_or(0);
        let max = if store.union_members(rest_ty).is_some() {
            // Union-rest mismatches are reported once against the tuple union.
            None
        } else if shapes.iter().any(|shape| shape.max_len().is_none()) {
            None
        } else {
            shapes.iter().filter_map(RestCallShape::max_len).max()
        };
        return RestArity { min, max };
    }
    RestArity { min: 0, max: None }
}

pub(super) fn call_argument_targets(
    interner: &mut Interner,
    params: &[ParameterType],
    arg_count: usize,
) -> Vec<Option<CallArgumentTarget>> {
    let alternatives = call_argument_target_alternatives(interner.store(), params, arg_count);
    let Some(first) = alternatives.first() else {
        return vec![None; arg_count];
    };
    if alternatives.len() == 1 {
        return first.clone();
    }
    (0..arg_count)
        .map(|index| {
            let targets: Vec<CallArgumentTarget> = alternatives
                .iter()
                .filter_map(|targets| targets.get(index).copied().flatten())
                .collect();
            let first = targets.first().copied()?;
            assert!(
                targets
                    .iter()
                    .all(|target| target.accepts_undefined == first.accepts_undefined),
                "call target alternatives disagree on optional fixed-slot metadata"
            );
            Some(CallArgumentTarget {
                declared: interner
                    .union(targets.into_iter().map(|target| target.declared).collect()),
                accepts_undefined: first.accepts_undefined,
            })
        })
        .collect()
}

pub(super) fn call_argument_target_alternatives(
    store: &Store,
    params: &[ParameterType],
    arg_count: usize,
) -> Vec<Vec<Option<CallArgumentTarget>>> {
    let fixed: Vec<&ParameterType> = params.iter().filter(|param| param.is_fixed()).collect();
    let rest = params.iter().find(|param| param.rest);
    let total_rest_args = arg_count.saturating_sub(fixed.len());
    let fixed_targets = || {
        (0..arg_count)
            .map(|index| {
                fixed.get(index).map(|param| CallArgumentTarget {
                    declared: param.ty,
                    accepts_undefined: param.optional,
                })
            })
            .collect::<Vec<_>>()
    };
    let Some(rest) = rest else {
        return vec![fixed_targets()];
    };
    let Some(shapes) = rest_call_shapes(store, rest.ty) else {
        return if store.union_members(rest.ty).is_some() {
            Vec::new()
        } else {
            vec![fixed_targets()]
        };
    };
    let alternatives: Vec<Vec<Option<CallArgumentTarget>>> = shapes
        .iter()
        .filter(|shape| shape.accepts_len(total_rest_args))
        .map(|shape| {
            (0..arg_count)
                .map(|index| {
                    if let Some(param) = fixed.get(index) {
                        return Some(CallArgumentTarget {
                            declared: param.ty,
                            accepts_undefined: param.optional,
                        });
                    }
                    shape
                        .element_at(index - fixed.len(), total_rest_args)
                        .map(|declared| CallArgumentTarget {
                            declared,
                            accepts_undefined: false,
                        })
                })
                .collect()
        })
        .collect();
    if !alternatives.is_empty() {
        return alternatives;
    }
    if store.union_members(rest.ty).is_some() {
        return Vec::new();
    }
    vec![(0..arg_count)
        .map(|index| {
            if let Some(param) = fixed.get(index) {
                return Some(CallArgumentTarget {
                    declared: param.ty,
                    accepts_undefined: param.optional,
                });
            }
            rest_argument_target(store, rest.ty, index - fixed.len(), total_rest_args).map(
                |declared| CallArgumentTarget {
                    declared,
                    accepts_undefined: false,
                },
            )
        })
        .collect()]
}

pub(super) fn contextual_parameter_target(
    interner: &mut Interner,
    params: &[ParameterType],
    index: usize,
    parameter_count: usize,
) -> Option<TypeId> {
    let fixed_count = params
        .iter()
        .filter(|parameter| parameter.is_fixed())
        .count();
    let minimum_count = params
        .iter()
        .find(|parameter| parameter.rest)
        .map(|rest| fixed_count + rest_parameter_arity(interner.store(), rest.ty).min)
        .unwrap_or(fixed_count);
    call_argument_targets(interner, params, parameter_count.max(minimum_count))
        .get(index)
        .copied()
        .flatten()
        .map(|target| target.declared)
}

pub(super) fn ready_contextual_parameter_target(
    interner: &mut Interner,
    params: &[ParameterType],
    index: usize,
    parameter_count: usize,
) -> Option<TypeId> {
    if let Some(parameter) = params
        .iter()
        .filter(|parameter| parameter.is_fixed())
        .nth(index)
    {
        return Some(parameter.ty);
    }
    if let Some(rest) = params.iter().find(|parameter| parameter.rest) {
        if !rest_container_is_ready(interner.store(), rest.ty) {
            return None;
        }
    }
    contextual_parameter_target(interner, params, index, parameter_count)
}

fn rest_container_is_ready(store: &Store, ty: TypeId) -> bool {
    let ty = store.readonly_operand(ty).unwrap_or(ty);
    if let Some(members) = store.union_members(ty) {
        return members.iter().all(|member| {
            let member = store.readonly_operand(*member).unwrap_or(*member);
            (store.array_type(member).is_some() || store.tuple_type(member).is_some())
                && rest_container_is_ready(store, member)
        });
    }
    if store.array_type(ty).is_some() {
        return true;
    }
    store.tuple_type(ty).is_some_and(|tuple| {
        tuple.rest.is_none_or(|rest| {
            rest.position <= tuple.elements.len() && rest_container_is_ready(store, rest.ty)
        })
    })
}

fn rest_argument_target(
    store: &Store,
    rest_ty: TypeId,
    offset: usize,
    total_rest_args: usize,
) -> Option<TypeId> {
    rest_call_shapes(store, rest_ty)?
        .into_iter()
        .find(|shape| shape.accepts_len(total_rest_args))?
        .element_at(offset, total_rest_args)
}

pub(super) fn rest_call_shapes(store: &Store, rest_ty: TypeId) -> Option<Vec<RestCallShape>> {
    let rest_ty = store.readonly_operand(rest_ty).unwrap_or(rest_ty);
    if let Some(members) = store.union_members(rest_ty) {
        let members = members.to_vec();
        let mut shapes = Vec::with_capacity(members.len());
        for member in members {
            let member = store.readonly_operand(member).unwrap_or(member);
            if store.array_type(member).is_none() && store.tuple_type(member).is_none() {
                return None;
            }
            shapes.extend(rest_call_shapes(store, member)?);
        }
        return Some(shapes);
    }
    if let Some(array) = store.array_type(rest_ty) {
        return Some(vec![RestCallShape {
            prefix: Vec::new(),
            variadic: Some(array.element),
            suffix: Vec::new(),
        }]);
    }
    if let Some(tuple) = store.tuple_type(rest_ty) {
        return tuple_call_shapes(store, tuple);
    }
    Some(vec![RestCallShape {
        prefix: Vec::new(),
        variadic: Some(rest_ty),
        suffix: Vec::new(),
    }])
}

fn tuple_call_shapes(store: &Store, tuple: &TupleType) -> Option<Vec<RestCallShape>> {
    let Some(rest) = tuple.rest else {
        return Some(vec![RestCallShape {
            prefix: tuple.elements.clone(),
            variadic: None,
            suffix: Vec::new(),
        }]);
    };
    if rest.position > tuple.elements.len() {
        return None;
    }
    let suffix = tuple.elements[rest.position..].to_vec();
    rest_call_shapes(store, rest.ty)?
        .into_iter()
        .map(|rest_shape| {
            let mut prefix = tuple.elements[..rest.position].to_vec();
            prefix.extend(rest_shape.prefix);
            let mut combined_suffix = rest_shape.suffix;
            combined_suffix.extend_from_slice(&suffix);
            RestCallShape {
                prefix,
                variadic: rest_shape.variadic,
                suffix: combined_suffix,
            }
        })
        .collect::<Vec<_>>()
        .into()
}

pub(super) fn tuple_context_element(
    store: &Store,
    tuple: &TupleType,
    index: usize,
    total_elements: usize,
) -> Option<TypeId> {
    tuple_call_shapes(store, tuple)?
        .into_iter()
        .find(|shape| shape.accepts_len(total_elements))?
        .element_at(index, total_elements)
}

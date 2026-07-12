//! Type-parameter substitution for generic instantiation.
//! Rewrites free declaration parameters through composite types and re-interns the
//! result, so equal instantiations share one `TypeId`. The `in_progress` guard
//! returns the original id on self-referential nominal types; recursive generic
//! instantiation remains out of scope.

use crate::types::repr::{FunctionType, TypeParamId, TypeTag};
use crate::types::store::TypeId;
use crate::types::Interner;
use rustc_hash::{FxHashMap, FxHashSet};

mod apply;
#[cfg(test)]
mod tests;

/// A substitution `TypeParamId → TypeId` plus the cycle guard, applied over the
/// type store. Built once per instantiation and dropped after.
pub struct Substitution<'a> {
    map: &'a FxHashMap<TypeParamId, TypeId>,
    /// Type ids currently being rewritten on the recursion stack — re-entry
    /// returns the original id, breaking a (nominal) cycle. See the module docs.
    in_progress: FxHashSet<TypeId>,
    /// Generic function binders currently crossed while applying an outer
    /// substitution. They shadow matching ids in `map` until that signature exits.
    blocked: FxHashSet<TypeParamId>,
}

impl<'a> Substitution<'a> {
    /// Build a substitution from a `TypeParamId → TypeId` map.
    pub fn new(map: &'a FxHashMap<TypeParamId, TypeId>) -> Self {
        Substitution {
            map,
            in_progress: FxHashSet::default(),
            blocked: FxHashSet::default(),
        }
    }

    /// Rewrite `ty`, replacing every type-parameter occurrence per the map and
    /// re-interning the result. Recurses through objects, functions, unions, and
    /// nested type parameters; an empty map (no parameters) leaves `ty` unchanged.
    pub fn apply(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
        // Nothing to substitute (a non-generic instantiation, or a fully-resolved
        // subtree) — return as-is. This also keeps the common path allocation-free.
        if self.map.is_empty() {
            return ty;
        }

        match interner.store().tag(ty) {
            // A type parameter: replace it with its argument if mapped; otherwise
            // (a parameter from an *outer* scope, not part of this substitution)
            // leave it untouched.
            TypeTag::TypeParam => {
                let param_id = interner.store().type_param(ty).map(|p| p.id);
                match param_id
                    .filter(|id| !self.blocked.contains(id))
                    .and_then(|id| self.map.get(&id).copied())
                {
                    Some(arg) => arg,
                    None => ty,
                }
            }
            // Intrinsics and literals contain no type parameter — identity.
            TypeTag::Intrinsic | TypeTag::Literal => ty,
            TypeTag::Object => self.apply_object(interner, ty),
            TypeTag::Function => self.apply_function(interner, ty),
            TypeTag::Union => self.apply_union(interner, ty),
            TypeTag::Intersection => self.apply_intersection(interner, ty),
            TypeTag::Array => self.apply_array(interner, ty),
            TypeTag::Tuple => self.apply_tuple(interner, ty),
            TypeTag::Readonly => self.apply_readonly(interner, ty),
            TypeTag::Conditional => self.apply_conditional(interner, ty),
            TypeTag::Instantiation => self.apply_instantiation(interner, ty),
            TypeTag::Mapped => self.apply_mapped(interner, ty),
            TypeTag::Template => self.apply_template(interner, ty),
            TypeTag::Keyof => self.apply_keyof(interner, ty),
            // An `infer` binder (M25) / a mapped-value placeholder (M26) is a **bound**
            // node-scoped variable, never a free declaration parameter — the no-capture
            // rule (ADR-0002): substitution must leave it alone (the evaluator resolves
            // it, not this pass).
            TypeTag::Infer | TypeTag::MappedValue => ty,
        }
    }
}

/// Whether a check-parameter argument **distributes** a distributive conditional (M25):
/// a union (per-member evaluation), `never` (→ `never`), or the `boolean` intrinsic
/// (expands to `true | false` first). A single other type evaluates once — the plain
/// rewrite path.
fn distributes_over(interner: &Interner, arg: TypeId) -> bool {
    let wk = interner.well_known();
    interner.store().tag(arg) == TypeTag::Union || arg == wk.never || arg == wk.boolean
}

/// Convenience: instantiate `ty` with the given `TypeParamId → TypeId` map in one
/// call (builds a fresh [`Substitution`], applies it, drops it). Equal calls
/// produce equal interned ids.
pub fn substitute(
    interner: &mut Interner,
    ty: TypeId,
    map: &FxHashMap<TypeParamId, TypeId>,
) -> TypeId {
    Substitution::new(map).apply(interner, ty)
}

/// Instantiate a generic function's own binders for one call candidate.
///
/// Unlike [`substitute`], this consumes the outer function's binder list while
/// preserving binders nested inside parameter or return types.
pub fn instantiate_function(
    interner: &mut Interner,
    ty: TypeId,
    map: &FxHashMap<TypeParamId, TypeId>,
) -> TypeId {
    let Some(function) = interner.store().function_type(ty) else {
        return ty;
    };
    let params = function.params.clone();
    let ret = function.ret;
    let mut substitution = Substitution::new(map);
    let params = params
        .into_iter()
        .map(|mut param| {
            param.ty = substitution.apply(interner, param.ty);
            param
        })
        .collect();
    let ret = substitution.apply(interner, ret);
    interner.intern_function(FunctionType {
        type_params: Vec::new(),
        params,
        ret,
    })
}

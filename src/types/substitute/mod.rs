//! Type-parameter substitution for generic instantiation.
//! Rewrites free declaration parameters through composite types and re-interns the
//! result, so equal instantiations share one `TypeId`. The `in_progress` guard
//! returns the original id on self-referential nominal types; recursive generic
//! instantiation remains out of scope.

use crate::types::repr::{FunctionType, TypeParamId, TypeTag};
use crate::types::store::TypeId;
use crate::types::Interner;
use rustc_hash::{FxHashMap, FxHashSet};

#[cfg(test)]
#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct SubstitutionMeasure {
    pub runs: u64,
    pub apply_visits: u64,
    pub type_id_repeats: u64,
    pub exact_context_repeats: u64,
    pub type_param_map_hits: u64,
    pub blocked_type_param_hits: u64,
    pub cycle_reentries: u64,
    pub completed_memo_hits: u64,
    pub completed_memo_entries: u64,
    pub cycle_tainted_skips: u64,
    seen_type_ids: FxHashSet<TypeId>,
    seen_contexts: FxHashSet<(TypeId, Vec<TypeParamId>)>,
}

#[cfg(test)]
impl std::fmt::Debug for SubstitutionMeasure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubstitutionMeasure")
            .field("runs", &self.runs)
            .field("apply_visits", &self.apply_visits)
            .field("type_id_repeats", &self.type_id_repeats)
            .field("exact_context_repeats", &self.exact_context_repeats)
            .field("type_param_map_hits", &self.type_param_map_hits)
            .field("blocked_type_param_hits", &self.blocked_type_param_hits)
            .field("cycle_reentries", &self.cycle_reentries)
            .field("completed_memo_hits", &self.completed_memo_hits)
            .field("completed_memo_entries", &self.completed_memo_entries)
            .field("cycle_tainted_skips", &self.cycle_tainted_skips)
            .finish()
    }
}

#[cfg(test)]
thread_local! {
    static SUBSTITUTION_MEASURE: std::cell::RefCell<SubstitutionMeasure> = std::cell::RefCell::new(SubstitutionMeasure::default());
}

#[cfg(test)]
pub(super) fn reset_substitution_measure() {
    SUBSTITUTION_MEASURE.with(|measure| *measure.borrow_mut() = SubstitutionMeasure::default());
}

#[cfg(test)]
pub(super) fn substitution_measure() -> SubstitutionMeasure {
    SUBSTITUTION_MEASURE.with(|measure| measure.borrow().clone())
}

#[cfg(test)]
fn measure_substitution_visit(ty: TypeId, blocked: &FxHashSet<TypeParamId>) {
    SUBSTITUTION_MEASURE.with(|measure| {
        let mut measure = measure.borrow_mut();
        measure.apply_visits += 1;
        if !measure.seen_type_ids.insert(ty) {
            measure.type_id_repeats += 1;
        }
        let mut context: Vec<TypeParamId> = blocked.iter().copied().collect();
        context.sort_unstable();
        if !measure.seen_contexts.insert((ty, context)) {
            measure.exact_context_repeats += 1;
        }
    });
}

#[cfg(test)]
pub(super) fn measure_substitution(update: impl FnOnce(&mut SubstitutionMeasure)) {
    SUBSTITUTION_MEASURE.with(|measure| update(&mut measure.borrow_mut()));
}

mod apply;
#[cfg(test)]
mod completed_memo_spec;
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
    /// Results completed without observing a recursive re-entry, scoped to this run.
    completed: FxHashMap<(TypeId, Vec<TypeParamId>), TypeId>,
    /// Incremented by every raw-`TypeId` re-entry to taint all active ancestors.
    cycle_epoch: u64,
}

impl<'a> Substitution<'a> {
    /// Build a substitution from a `TypeParamId → TypeId` map.
    pub fn new(map: &'a FxHashMap<TypeParamId, TypeId>) -> Self {
        #[cfg(test)]
        measure_substitution(|measure| measure.runs += 1);
        Substitution {
            map,
            in_progress: FxHashSet::default(),
            blocked: FxHashSet::default(),
            completed: FxHashMap::default(),
            cycle_epoch: 0,
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

        #[cfg(test)]
        measure_substitution_visit(ty, &self.blocked);

        // Raw-id re-entry must win over a completed result under any blocked
        // context; returning the original id is the existing cycle semantics.
        if self.in_progress.contains(&ty) {
            self.cycle_epoch += 1;
            #[cfg(test)]
            measure_substitution(|measure| measure.cycle_reentries += 1);
            return ty;
        }

        let key = (ty, self.canonical_blocked_context());
        if let Some(&result) = self.completed.get(&key) {
            #[cfg(test)]
            measure_substitution(|measure| measure.completed_memo_hits += 1);
            return result;
        }

        let start_cycle_epoch = self.cycle_epoch;
        let result = match interner.store().tag(ty) {
            // A type parameter: replace it with its argument if mapped; otherwise
            // (a parameter from an *outer* scope, not part of this substitution)
            // leave it untouched.
            TypeTag::TypeParam => {
                let param_id = interner.store().type_param(ty).map(|p| p.id);
                #[cfg(test)]
                if param_id.is_some_and(|id| self.blocked.contains(&id)) {
                    measure_substitution(|measure| measure.blocked_type_param_hits += 1);
                }
                match param_id
                    .filter(|id| !self.blocked.contains(id))
                    .and_then(|id| self.map.get(&id).copied())
                {
                    Some(arg) => {
                        #[cfg(test)]
                        measure_substitution(|measure| measure.type_param_map_hits += 1);
                        arg
                    }
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
            TypeTag::ClassInstance => self.apply_class_instance(interner, ty),
            TypeTag::Mapped => self.apply_mapped(interner, ty),
            TypeTag::Template => self.apply_template(interner, ty),
            TypeTag::Keyof => self.apply_keyof(interner, ty),
            TypeTag::DeferredIndexedAccess => self.apply_deferred_indexed_access(interner, ty),
            // An `infer` binder (M25) / a mapped-value placeholder (M26) is a **bound**
            // node-scoped variable, never a free declaration parameter — the no-capture
            // rule (ADR-0002): substitution must leave it alone (the evaluator resolves
            // it, not this pass).
            TypeTag::Infer | TypeTag::MappedValue => ty,
        };

        if self.cycle_epoch == start_cycle_epoch {
            self.completed.insert(key, result);
            #[cfg(test)]
            measure_substitution(|measure| measure.completed_memo_entries += 1);
        } else {
            #[cfg(test)]
            measure_substitution(|measure| measure.cycle_tainted_skips += 1);
        }

        result
    }

    fn canonical_blocked_context(&self) -> Vec<TypeParamId> {
        let mut context: Vec<TypeParamId> = self
            .blocked
            .iter()
            .filter(|id| self.map.contains_key(id))
            .copied()
            .collect();
        context.sort_unstable();
        context
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
    let receiver = function.receiver;
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
    let receiver = receiver.map(|receiver| substitution.apply(interner, receiver));
    interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver,
        params,
        ret,
    })
}

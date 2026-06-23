//! Type-parameter substitution (M9 — generics core).
//!
//! Instantiation of a generic declaration is **substitution**: given a type and a
//! map `TypeParamId → TypeId`, produce the type with every type-parameter
//! occurrence replaced by its argument, recursing through objects, functions,
//! unions, and nested type parameters, then re-interning the result so equal
//! instantiations share one `TypeId` (`Box<number>` is consistent;
//! `Box<number>` ≠ `Box<string>`).
//!
//! This is the statement-checker-facing half of M9; the relation engine then runs
//! over the *substituted* (structural) types unchanged. For a generic interface,
//! instantiating to the substituted **structural** type is the M9 choice (the
//! README's "structural assignability is acceptable for M9" — see the FLAG in
//! `checker.rs::instantiate_type_reference`).
//!
//! ## Cycle safety
//!
//! The store can hold self-referential **nominal** types (M5 — `interface List {
//! tail: List | null }`, whose `tail` references `List`'s own id). A naive walk
//! would chase such a cycle forever. [`Substitution`] therefore carries an
//! `in_progress` set of the type ids currently being rewritten; re-entering one
//! short-circuits by returning the **original** id. This is sound here because the
//! only types that legitimately reference themselves by id are nominal types that
//! contain **no in-scope type parameter** (the M9 generic-interface bodies the
//! fixtures use are *non-recursive* structural objects), so their substitution is
//! the identity — returning the original id is exactly the fixpoint. The guard is
//! defensive against an accidental recursive instantiation rather than a feature:
//! recursive generic instantiation (and its depth limits) is explicitly out of the
//! M9 scope.

use crate::types::repr::{FunctionType, ObjectType, ParameterType, PropertyType, TypeParamId, TypeTag};
use crate::types::store::TypeId;
use crate::types::Interner;
use rustc_hash::{FxHashMap, FxHashSet};

/// A substitution `TypeParamId → TypeId` plus the cycle guard, applied over the
/// type store. Built once per instantiation and dropped after.
pub struct Substitution<'a> {
    map: &'a FxHashMap<TypeParamId, TypeId>,
    /// Type ids currently being rewritten on the recursion stack — re-entry
    /// returns the original id, breaking a (nominal) cycle. See the module docs.
    in_progress: FxHashSet<TypeId>,
}

impl<'a> Substitution<'a> {
    /// Build a substitution from a `TypeParamId → TypeId` map.
    pub fn new(map: &'a FxHashMap<TypeParamId, TypeId>) -> Self {
        Substitution {
            map,
            in_progress: FxHashSet::default(),
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
                match param_id.and_then(|id| self.map.get(&id).copied()) {
                    Some(arg) => arg,
                    None => ty,
                }
            }
            // Intrinsics and literals contain no type parameter — identity.
            TypeTag::Intrinsic | TypeTag::Literal => ty,
            TypeTag::Object => self.apply_object(interner, ty),
            TypeTag::Function => self.apply_function(interner, ty),
            TypeTag::Union => self.apply_union(interner, ty),
        }
    }

    /// Substitute through an object type, rewriting each property type and
    /// re-interning the (structural) result **only when a child actually changed**.
    /// If no property substitutes, the original id is returned unchanged — which
    /// preserves a *nominal* object's identity (it is never re-interned into a
    /// fresh structural copy) and keeps the no-op path allocation-free.
    /// Cycle-guarded for self-referential nominal objects (returns the original id
    /// on re-entry).
    fn apply_object(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
        // Re-entry on an in-flight object id breaks the cycle (see module docs).
        if self.in_progress.contains(&ty) {
            return ty;
        }
        // Snapshot the property (name, optional, type) tuples before any mutable
        // re-intern: reading the side-table borrows the store immutably, while the
        // recursive `apply` and the final `intern_object` need `&mut`.
        let Some(object) = interner.store().object_type(ty) else {
            return ty;
        };
        let props: Vec<(String, bool, TypeId)> = object
            .properties
            .iter()
            .map(|p| (p.name.clone(), p.optional, p.ty))
            .collect();

        self.in_progress.insert(ty);
        let mut changed = false;
        let properties: Vec<PropertyType> = props
            .into_iter()
            .map(|(name, optional, prop_ty)| {
                let new_ty = self.apply(interner, prop_ty);
                changed |= new_ty != prop_ty;
                PropertyType {
                    name,
                    ty: new_ty,
                    optional,
                }
            })
            .collect();
        self.in_progress.remove(&ty);

        // Unchanged → keep the original id (preserves nominal identity); changed →
        // intern the substituted structural object.
        if changed {
            interner.intern_object(ObjectType { properties })
        } else {
            ty
        }
    }

    /// Substitute through a function type, rewriting each parameter type and the
    /// return type, then re-interning **only when something changed** (else the
    /// original id is returned). Parameters stay positional.
    fn apply_function(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
        if self.in_progress.contains(&ty) {
            return ty;
        }
        let Some(function) = interner.store().function_type(ty) else {
            return ty;
        };
        let params: Vec<(String, bool, TypeId)> = function
            .params
            .iter()
            .map(|p| (p.name.clone(), p.optional, p.ty))
            .collect();
        let ret = function.ret;

        self.in_progress.insert(ty);
        let mut changed = false;
        let lowered: Vec<ParameterType> = params
            .into_iter()
            .map(|(name, optional, param_ty)| {
                let new_ty = self.apply(interner, param_ty);
                changed |= new_ty != param_ty;
                ParameterType {
                    name,
                    ty: new_ty,
                    optional,
                }
            })
            .collect();
        let new_ret = self.apply(interner, ret);
        changed |= new_ret != ret;
        self.in_progress.remove(&ty);

        if changed {
            interner.intern_function(FunctionType {
                params: lowered,
                ret: new_ret,
            })
        } else {
            ty
        }
    }

    /// Substitute through a union, rewriting each member and re-interning through
    /// `Interner::union` (so the result is re-canonicalized: a member that
    /// substitutes to a duplicate or to `never` collapses correctly) **only when a
    /// member changed**; otherwise the original id is returned.
    fn apply_union(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
        if self.in_progress.contains(&ty) {
            return ty;
        }
        let Some(members) = interner.store().union_members(ty) else {
            return ty;
        };
        let members: Vec<TypeId> = members.to_vec();

        self.in_progress.insert(ty);
        let mut changed = false;
        let substituted: Vec<TypeId> = members
            .iter()
            .map(|&m| {
                let new_m = self.apply(interner, m);
                changed |= new_m != m;
                new_m
            })
            .collect();
        self.in_progress.remove(&ty);

        if changed {
            interner.union(substituted)
        } else {
            ty
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::repr::{ObjectType, PropertyType};

    fn prop(name: &str, ty: TypeId) -> PropertyType {
        PropertyType {
            name: name.to_string(),
            ty,
            optional: false,
        }
    }

    /// A bare type parameter is replaced by its argument; an unmapped parameter is
    /// left untouched.
    #[test]
    fn type_param_is_replaced() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let t = interner.intern_type_param(TypeParamId(0), "T");
        let u = interner.intern_type_param(TypeParamId(1), "U");

        let mut map = FxHashMap::default();
        map.insert(TypeParamId(0), wk.number);

        assert_eq!(substitute(&mut interner, t, &map), wk.number, "T → number");
        assert_eq!(substitute(&mut interner, u, &map), u, "unmapped U is untouched");
        // An intrinsic is unaffected.
        assert_eq!(substitute(&mut interner, wk.string, &map), wk.string);
    }

    /// Substitution recurses everywhere: a type parameter nested inside an object,
    /// a function, and a union is all replaced.
    #[test]
    fn substitution_recurses_into_object_function_union() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let t = interner.intern_type_param(TypeParamId(0), "T");

        // { value: T }
        let box_t = interner.intern_object(ObjectType {
            properties: vec![prop("value", t)],
        });
        // (x: T) => T
        let fn_t = interner.intern_function(FunctionType {
            params: vec![ParameterType {
                name: "x".to_string(),
                ty: t,
                optional: false,
            }],
            ret: t,
        });
        // T | null
        let t_or_null = interner.union(vec![t, wk.null]);

        let mut map = FxHashMap::default();
        map.insert(TypeParamId(0), wk.number);

        // { value: number }
        let box_num = interner.intern_object(ObjectType {
            properties: vec![prop("value", wk.number)],
        });
        let fn_num = interner.intern_function(FunctionType {
            params: vec![ParameterType {
                name: "x".to_string(),
                ty: wk.number,
                optional: false,
            }],
            ret: wk.number,
        });
        let num_or_null = interner.union(vec![wk.number, wk.null]);

        assert_eq!(substitute(&mut interner, box_t, &map), box_num);
        assert_eq!(substitute(&mut interner, fn_t, &map), fn_num);
        assert_eq!(substitute(&mut interner, t_or_null, &map), num_or_null);
    }

    /// Nested substitution flows through `Box<Box<number>>`-style nesting: the
    /// outer object's property is itself an object referencing the parameter.
    #[test]
    fn substitution_flows_through_nested_objects() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let t = interner.intern_type_param(TypeParamId(0), "T");

        // { value: { value: T } }
        let inner = interner.intern_object(ObjectType {
            properties: vec![prop("value", t)],
        });
        let outer = interner.intern_object(ObjectType {
            properties: vec![prop("value", inner)],
        });

        // Instantiate with T → { value: number }.
        let box_num = interner.intern_object(ObjectType {
            properties: vec![prop("value", wk.number)],
        });
        let mut map = FxHashMap::default();
        map.insert(TypeParamId(0), box_num);

        // inner = { value: T }  →  { value: { value: number } }
        let inner_subst = interner.intern_object(ObjectType {
            properties: vec![prop("value", box_num)],
        });
        // outer = { value: inner } = { value: { value: T } }  →
        //         { value: { value: { value: number } } }
        let outer_subst = interner.intern_object(ObjectType {
            properties: vec![prop("value", inner_subst)],
        });
        assert_eq!(substitute(&mut interner, inner, &map), inner_subst);
        assert_eq!(substitute(&mut interner, outer, &map), outer_subst);
    }

    /// Two distinct instantiations are distinct interned types, and the same
    /// instantiation re-interns to the same id (instantiation interning is
    /// consistent): `Box<number>` interns consistently and `Box<number>` ≠
    /// `Box<string>`.
    #[test]
    fn instantiation_interning_is_consistent_and_distinct() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let t = interner.intern_type_param(TypeParamId(0), "T");
        let box_t = interner.intern_object(ObjectType {
            properties: vec![prop("value", t)],
        });

        let mut num_map = FxHashMap::default();
        num_map.insert(TypeParamId(0), wk.number);
        let mut str_map = FxHashMap::default();
        str_map.insert(TypeParamId(0), wk.string);

        let box_num_a = substitute(&mut interner, box_t, &num_map);
        let box_num_b = substitute(&mut interner, box_t, &num_map);
        let box_str = substitute(&mut interner, box_t, &str_map);

        assert_eq!(box_num_a, box_num_b, "Box<number> interns consistently");
        assert_ne!(box_num_a, box_str, "Box<number> ≠ Box<string>");
    }

    /// A self-referential **nominal** object (no type parameter) substitutes to
    /// itself and **terminates** — the cycle guard returns the original id on
    /// re-entry rather than looping.
    #[test]
    fn self_referential_nominal_object_terminates() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();

        // A recursive nominal interface `List { head: number; tail: List | null }`.
        let list = interner.reserve_object();
        let list_or_null = interner.union(vec![list, wk.null]);
        interner.fill_object(
            list,
            ObjectType {
                properties: vec![prop("head", wk.number), prop("tail", list_or_null)],
            },
        );

        // Substituting T → string over the recursive `list` must terminate. The
        // list has no type parameter, so the result is the list itself (its nominal
        // identity is preserved — it is not re-interned into a structural copy).
        let mut map = FxHashMap::default();
        map.insert(TypeParamId(0), wk.string);
        assert_eq!(
            substitute(&mut interner, list, &map),
            list,
            "a recursive nominal object with no type param substitutes to itself"
        );
    }
}

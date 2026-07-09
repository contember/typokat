//! Type-parameter substitution for generic instantiation.
//! Rewrites free declaration parameters through composite types and re-interns the
//! result, so equal instantiations share one `TypeId`. The `in_progress` guard
//! returns the original id on self-referential nominal types; recursive generic
//! instantiation remains out of scope.

use crate::types::repr::{
    ConditionalType, FunctionType, MappedType, ObjectType, ParameterType, PropertyType,
    TemplateType, TypeParamId, TypeTag,
};
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

    /// Substitute through an object, re-interning only when a child changed.
    /// No-op substitution preserves nominal identity; re-entry returns the original
    /// id to break self-referential nominal cycles.
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
        // M13: carry the member's visibility + declaring class through unchanged —
        // substitution rewrites a member's *type*, never its access modifier or
        // origin (so a substituted generic class member, were generic classes in
        // scope, would keep its nominal identity).
        let props: Vec<PropertyType> = object.properties.clone();
        // M19: snapshot the index-signature value types so they too are rewritten
        // (a generic `{ [k: string]: T }` instantiates to `{ [k: string]: number }`).
        let (string_index, number_index) = (object.string_index, object.number_index);
        // F1/WU2/WU3: snapshot call/construct signatures; each is an interned
        // FunctionType id and must be recursively substituted just like a
        // function-typed property.
        let call_signatures = object.call_signatures.clone();
        let construct_signatures = object.construct_signatures.clone();

        self.in_progress.insert(ty);
        let mut changed = false;
        let properties: Vec<PropertyType> = props
            .into_iter()
            .map(|p| {
                let new_ty = self.apply(interner, p.ty);
                changed |= new_ty != p.ty;
                PropertyType { ty: new_ty, ..p }
            })
            .collect();
        // M19: rewrite each index signature's value type through the same recursion.
        let string_index = string_index.map(|v| {
            let new_v = self.apply(interner, v);
            changed |= new_v != v;
            new_v
        });
        let number_index = number_index.map(|v| {
            let new_v = self.apply(interner, v);
            changed |= new_v != v;
            new_v
        });
        let call_signatures: Vec<TypeId> = call_signatures
            .into_iter()
            .map(|signature| {
                let new_signature = self.apply(interner, signature);
                changed |= new_signature != signature;
                new_signature
            })
            .collect();
        let construct_signatures: Vec<TypeId> = construct_signatures
            .into_iter()
            .map(|signature| {
                let new_signature = self.apply(interner, signature);
                changed |= new_signature != signature;
                new_signature
            })
            .collect();
        self.in_progress.remove(&ty);

        // Unchanged → keep the original id (preserves nominal identity); changed →
        // intern the substituted structural object.
        if changed {
            interner.intern_object(ObjectType {
                properties,
                string_index,
                number_index,
                call_signatures,
                construct_signatures,
            })
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

    /// Substitute through an intersection and re-canonicalize through
    /// `Interner::intersection` only when a member changed.
    fn apply_intersection(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
        if self.in_progress.contains(&ty) {
            return ty;
        }
        let Some(members) = interner.store().intersection_members(ty) else {
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
            interner.intersection(substituted)
        } else {
            ty
        }
    }

    /// Substitute an array's element and re-intern only when it changed. No cycle
    /// guard is needed because the element is an interned child id.
    fn apply_array(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
        let Some(array) = interner.store().array_type(ty) else {
            return ty;
        };
        let element = array.element;
        let new_element = self.apply(interner, element);
        if new_element != element {
            interner.intern_array(new_element)
        } else {
            ty
        }
    }

    /// Substitute tuple elements positionally and re-intern only when one changed.
    /// Element order is preserved; child ids make the recursion finite.
    fn apply_tuple(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
        let Some(tuple) = interner.store().tuple_type(ty) else {
            return ty;
        };
        let elements: Vec<TypeId> = tuple.elements.clone();

        let mut changed = false;
        let substituted: Vec<TypeId> = elements
            .iter()
            .map(|&e| {
                let new_e = self.apply(interner, e);
                changed |= new_e != e;
                new_e
            })
            .collect();

        if changed {
            interner.intern_tuple(substituted)
        } else {
            ty
        }
    }

    fn apply_readonly(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
        let Some(operand) = interner.store().readonly_operand(ty) else {
            return ty;
        };
        let new_operand = self.apply(interner, operand);
        if new_operand != operand {
            interner.intern_readonly(new_operand)
        } else {
            ty
        }
    }

    /// Substitute a conditional's four component ids and re-intern only on change.
    /// This plain rewrite never captures the node's own `infer` binders (ADR-0002);
    /// recursive conditional aliases stay behind lazy instantiation nodes.
    ///
    /// **Distribution guard**: if the naked check parameter of a distributive
    /// conditional maps to a union, `never`, or `boolean`, plain rewriting would
    /// evaluate the whole union once instead of per member. Defer as a lazy
    /// [`crate::types::repr::InstantiationType`] so the evaluator distributes on the
    /// same path as alias instantiation. Single non-distributing arguments take the
    /// plain rewrite, preventing evaluator per-member substitutions from re-wrapping.
    fn apply_conditional(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
        let Some(cond) = interner.store().conditional_type(ty).copied() else {
            return ty;
        };
        // A poisoned node never evaluates (backlog 26 stopgap), so the lazy distribution
        // wrap below would be pointless — it takes the plain rewrite (carrying the flag).
        if cond.distributive && !cond.poisoned {
            let mapped = interner
                .store()
                .type_param(cond.check)
                .map(|p| p.id)
                .and_then(|id| self.map.get(&id).copied());
            if let Some(arg) = mapped {
                if distributes_over(interner, arg) {
                    let args: Vec<(TypeParamId, TypeId)> =
                        self.map.iter().map(|(&p, &v)| (p, v)).collect();
                    return interner.intern_instantiation(ty, args);
                }
            }
        }
        let check = self.apply(interner, cond.check);
        let extends_ty = self.apply(interner, cond.extends_ty);
        let true_branch = self.apply(interner, cond.true_branch);
        let false_branch = self.apply(interner, cond.false_branch);
        if check == cond.check
            && extends_ty == cond.extends_ty
            && true_branch == cond.true_branch
            && false_branch == cond.false_branch
        {
            return ty;
        }
        interner.intern_conditional(ConditionalType {
            check,
            extends_ty,
            true_branch,
            false_branch,
            infer_count: cond.infer_count,
            distributive: cond.distributive,
            poisoned: cond.poisoned,
        })
    }

    /// Substitute through a lazy instantiation by composing into argument values
    /// only. The base and argument keys stay untouched, keeping recursive references
    /// lazy; re-intern only when an argument value changed.
    fn apply_instantiation(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
        let Some(inst) = interner.store().instantiation_type(ty) else {
            return ty;
        };
        let base = inst.base;
        let args: Vec<(TypeParamId, TypeId)> = inst.args.clone();
        let mut changed = false;
        let new_args: Vec<(TypeParamId, TypeId)> = args
            .into_iter()
            .map(|(param, value)| {
                let new_value = self.apply(interner, value);
                changed |= new_value != value;
                (param, new_value)
            })
            .collect();
        if changed {
            interner.intern_instantiation(base, new_args)
        } else {
            ty
        }
    }

    /// Substitute a mapped type's key source and value template, re-interning only
    /// on change. `MappedValue` is a bound placeholder (no-capture); free
    /// declaration parameters in the key source still rewrite.
    ///
    /// **Mapped distribution guard**: a homomorphic map whose `keyof` operand is a
    /// naked declaration parameter mapped to a union distributes per member
    /// (`Ident<A | B>` = `Ident<A> | Ident<B>`); `never` distributes to `never`.
    /// This eagerly builds a union because mapped nodes are already lazy evaluation
    /// units. The direct `{ [K in keyof (A | B)]: … }` form is different: its key
    /// source is already a union at lowering and uses evaluation-time common-key
    /// semantics, so it must not trigger this guard.
    fn apply_mapped(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
        let Some(mapped) = interner.store().mapped_type(ty).copied() else {
            return ty;
        };
        if mapped.homomorphic {
            let param_id = interner.store().type_param(mapped.key_source).map(|p| p.id);
            if let Some((param, arg)) =
                param_id.and_then(|id| self.map.get(&id).copied().map(|arg| (id, arg)))
            {
                let wk = interner.well_known();
                if arg == wk.never {
                    // Distributing over zero members: Ident<never> = never (tsc).
                    return wk.never;
                }
                if let Some(members) = interner.store().union_members(arg) {
                    let members: Vec<TypeId> = members.to_vec();
                    let per_member: Vec<TypeId> = members
                        .into_iter()
                        .map(|member| {
                            let mut member_map = self.map.clone();
                            member_map.insert(param, member);
                            substitute(interner, ty, &member_map)
                        })
                        .collect();
                    return interner.union(per_member);
                }
            }
        }
        let key_source = self.apply(interner, mapped.key_source);
        let value_template = self.apply(interner, mapped.value_template);
        // M28: the modifiers source (`T` of a captured `T[P]`) is a free-position
        // component like the key source — substitution rewrites it so `Pick<P, …>`
        // resolves each key against the concrete `P`.
        let modifiers_source = mapped.modifiers_source.map(|ms| self.apply(interner, ms));
        if key_source == mapped.key_source
            && value_template == mapped.value_template
            && modifiers_source == mapped.modifiers_source
        {
            return ty;
        }
        interner.intern_mapped(MappedType {
            homomorphic: mapped.homomorphic,
            key_source,
            value_template,
            modifiers_source,
            optional_modifier: mapped.optional_modifier,
            readonly_modifier: mapped.readonly_modifier,
        })
    }

    /// Substitute a deferred `keyof` operand and re-intern only on change. No eager
    /// evaluation here; the shared evaluator resolves concrete operands at demand.
    fn apply_keyof(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
        let Some(operand) = interner.store().keyof_operand(ty) else {
            return ty;
        };
        let new_operand = self.apply(interner, operand);
        if new_operand != operand {
            interner.intern_keyof(new_operand)
        } else {
            ty
        }
    }

    /// Substitute template holes and re-intern only when a hole changed. Text
    /// segments are untouched; literal/union construction remains evaluator work.
    fn apply_template(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
        let Some(template) = interner.store().template_type(ty) else {
            return ty;
        };
        let texts = template.texts.clone();
        let holes: Vec<TypeId> = template.holes.clone();

        let mut changed = false;
        let new_holes: Vec<TypeId> = holes
            .iter()
            .map(|&hole| {
                let new_hole = self.apply(interner, hole);
                changed |= new_hole != hole;
                new_hole
            })
            .collect();

        if changed {
            interner.intern_template(TemplateType {
                texts,
                holes: new_holes,
            })
        } else {
            ty
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::repr::{ObjectType, PropertyType};

    fn prop(name: &str, ty: TypeId) -> PropertyType {
        PropertyType::public(name, ty)
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
        assert_eq!(
            substitute(&mut interner, u, &map),
            u,
            "unmapped U is untouched"
        );
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
            ..Default::default()
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
            ..Default::default()
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

    /// Substitution rewrites an array's element (M17): `T[]` → `number[]`, nested
    /// `T[][]` → `number[][]`, and an unmapped element leaves the array untouched
    /// (the no-op path returns the original id).
    #[test]
    fn substitution_rewrites_array_element() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let t = interner.intern_type_param(TypeParamId(0), "T");
        let u = interner.intern_type_param(TypeParamId(1), "U");

        // T[] and T[][].
        let arr_t = interner.intern_array(t);
        let arr_arr_t = interner.intern_array(arr_t);
        // U[] (its element is not in the map).
        let arr_u = interner.intern_array(u);

        let mut map = FxHashMap::default();
        map.insert(TypeParamId(0), wk.number);

        let arr_num = interner.intern_array(wk.number);
        let arr_arr_num = interner.intern_array(arr_num);

        assert_eq!(
            substitute(&mut interner, arr_t, &map),
            arr_num,
            "T[] → number[]"
        );
        assert_eq!(
            substitute(&mut interner, arr_arr_t, &map),
            arr_arr_num,
            "T[][] → number[][]"
        );
        assert_eq!(
            substitute(&mut interner, arr_u, &map),
            arr_u,
            "U[] (unmapped element) is unchanged"
        );
    }

    /// Substitution rewrites each tuple element **positionally** (M18): `[T, U]` →
    /// `[number, string]`, order preserved; an unmapped element survives; a tuple
    /// with no mapped element is returned unchanged (no-op path).
    #[test]
    fn substitution_rewrites_tuple_elements_positionally() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let t = interner.intern_type_param(TypeParamId(0), "T");
        let u = interner.intern_type_param(TypeParamId(1), "U");

        // [T, U] and [U, T] (order matters).
        let tu = interner.intern_tuple(vec![t, u]);
        let ut = interner.intern_tuple(vec![u, t]);
        // [string, T] — only T is mapped.
        let str_t = interner.intern_tuple(vec![wk.string, t]);
        // [string, boolean] — no mapped element (unchanged under T → number).
        let str_bool = interner.intern_tuple(vec![wk.string, wk.boolean]);

        let mut map = FxHashMap::default();
        map.insert(TypeParamId(0), wk.number);
        map.insert(TypeParamId(1), wk.string);

        // [T, U] → [number, string]; [U, T] → [string, number] (order preserved).
        let num_str = interner.intern_tuple(vec![wk.number, wk.string]);
        let str_num = interner.intern_tuple(vec![wk.string, wk.number]);
        assert_eq!(
            substitute(&mut interner, tu, &map),
            num_str,
            "[T, U] → [number, string]"
        );
        assert_eq!(
            substitute(&mut interner, ut, &map),
            str_num,
            "[U, T] → [string, number] (positional, order preserved)"
        );

        // [string, T] → [string, number] (only the mapped element rewritten).
        let str_num_via = interner.intern_tuple(vec![wk.string, wk.number]);
        assert_eq!(substitute(&mut interner, str_t, &map), str_num_via);

        // No mapped element → unchanged id (no-op path returns the original).
        assert_eq!(
            substitute(&mut interner, str_bool, &map),
            str_bool,
            "a tuple with no mapped element is unchanged"
        );
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
            ..Default::default()
        });
        let outer = interner.intern_object(ObjectType {
            properties: vec![prop("value", inner)],
            ..Default::default()
        });

        // Instantiate with T → { value: number }.
        let box_num = interner.intern_object(ObjectType {
            properties: vec![prop("value", wk.number)],
            ..Default::default()
        });
        let mut map = FxHashMap::default();
        map.insert(TypeParamId(0), box_num);

        // inner = { value: T }  →  { value: { value: number } }
        let inner_subst = interner.intern_object(ObjectType {
            properties: vec![prop("value", box_num)],
            ..Default::default()
        });
        // outer = { value: inner } = { value: { value: T } }  →
        //         { value: { value: { value: number } } }
        let outer_subst = interner.intern_object(ObjectType {
            properties: vec![prop("value", inner_subst)],
            ..Default::default()
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
            ..Default::default()
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

    /// M25: substitution must not capture conditional `infer` binders, even under
    /// nesting; only free declaration parameters rewrite.
    #[test]
    fn substitute_does_not_capture_infer_binders_under_nesting() {
        use crate::types::repr::ConditionalType;

        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let t = interner.intern_type_param(TypeParamId(0), "T");
        let infer0 = interner.intern_infer(0);
        let arr_infer0 = interner.intern_array(infer0);

        // Inner: `T extends (infer U)[] ? U : T` (infer in extends + true, T in check/false).
        let inner = interner.intern_conditional(ConditionalType {
            check: t,
            extends_ty: arr_infer0,
            true_branch: infer0,
            false_branch: t,
            infer_count: 1,
            distributive: true,
            poisoned: false,
        });
        // Outer nests the inner conditional as the true branch: `T extends string ? <inner> : T`.
        let outer = interner.intern_conditional(ConditionalType {
            check: t,
            extends_ty: wk.string,
            true_branch: inner,
            false_branch: t,
            infer_count: 0,
            distributive: true,
            poisoned: false,
        });

        let mut map = FxHashMap::default();
        map.insert(TypeParamId(0), wk.number);
        let result = substitute(&mut interner, outer, &map);

        // The result: `number extends string ? <inner'> : number` where inner' is
        // `number extends (infer U)[] ? U : number` — the infer nodes UNCHANGED.
        let store = interner.store();
        let outer_c = store.conditional_type(result).expect("outer is conditional");
        assert_eq!(outer_c.check, wk.number, "T → number in outer check");
        assert_eq!(outer_c.false_branch, wk.number, "T → number in outer false");
        let inner_c = store
            .conditional_type(outer_c.true_branch)
            .expect("inner is conditional");
        assert_eq!(inner_c.check, wk.number, "T → number in inner check");
        assert_eq!(inner_c.false_branch, wk.number, "T → number in inner false");
        // No capture: the infer binder is untouched in both extends and true positions.
        assert_eq!(inner_c.extends_ty, arr_infer0, "infer extends unchanged (no capture)");
        assert_eq!(inner_c.true_branch, infer0, "infer true branch unchanged (no capture)");
    }

    /// M25 distribution guard: union/`never`/`boolean` check arguments defer to lazy
    /// instantiation; single non-distributing arguments plainly rewrite.
    #[test]
    fn distributive_conditional_defers_on_union_plain_on_single() {
        use crate::types::repr::ConditionalType;

        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let t = interner.intern_type_param(TypeParamId(0), "T");
        let yes = interner.intern_literal(crate::types::repr::LiteralValue::String("yes".into()));
        let no = interner.intern_literal(crate::types::repr::LiteralValue::String("no".into()));

        // `T extends string ? "yes" : "no"` (distributive — naked check param).
        let cond = interner.intern_conditional(ConditionalType {
            check: t,
            extends_ty: wk.string,
            true_branch: yes,
            false_branch: no,
            infer_count: 0,
            distributive: true,
            poisoned: false,
        });

        // A union argument defers as an instantiation of the ORIGINAL node.
        let union = interner.union(vec![wk.string, wk.number]);
        let mut map = FxHashMap::default();
        map.insert(TypeParamId(0), union);
        let deferred = substitute(&mut interner, cond, &map);
        let inst = interner
            .store()
            .instantiation_type(deferred)
            .expect("a union check argument must defer as a lazy instantiation");
        assert_eq!(inst.base, cond, "the instantiation wraps the original node");
        assert_eq!(inst.args, vec![(TypeParamId(0), union)]);

        // `never` distributes too (→ never at evaluation), so it must also defer.
        let mut never_map = FxHashMap::default();
        never_map.insert(TypeParamId(0), wk.never);
        let deferred_never = substitute(&mut interner, cond, &never_map);
        assert!(
            interner.store().instantiation_type(deferred_never).is_some(),
            "a `never` check argument must defer (it distributes to `never`)"
        );

        // A single non-distributing argument takes the PLAIN rewrite — a concrete
        // conditional, never a wrap (the evaluator's per-member path relies on this).
        let mut single_map = FxHashMap::default();
        single_map.insert(TypeParamId(0), wk.string);
        let plain = substitute(&mut interner, cond, &single_map);
        let plain_cond = interner
            .store()
            .conditional_type(plain)
            .expect("a single check argument must plainly rewrite to a concrete conditional");
        assert_eq!(plain_cond.check, wk.string);
    }

    /// M26: substitution rewrites mapped key/value templates without capturing the
    /// node's own `MappedValue` placeholder.
    #[test]
    fn substitute_maps_over_mapped_without_capturing_value_placeholder() {
        use crate::types::repr::{MappedType, ModifierOp};

        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let t = interner.intern_type_param(TypeParamId(0), "T");
        let value_placeholder = interner.intern_mapped_value();
        // Value template `T[K] | null` — the placeholder inside a union.
        let value_template = interner.union(vec![value_placeholder, wk.null]);

        // `{ [K in keyof T]: T[K] | null }` — key source is the bare parameter `T`.
        let mapped = interner.intern_mapped(MappedType {
            homomorphic: true,
            key_source: t,
            value_template,
            modifiers_source: None,
            optional_modifier: ModifierOp::Keep,
            readonly_modifier: ModifierOp::Keep,
        });

        // The concrete source `P = { a: number }`.
        let p = interner.intern_object(ObjectType {
            properties: vec![prop("a", wk.number)],
            ..Default::default()
        });
        let mut map = FxHashMap::default();
        map.insert(TypeParamId(0), p);

        let result = substitute(&mut interner, mapped, &map);
        let out = interner
            .store()
            .mapped_type(result)
            .copied()
            .expect("result is a mapped type");
        assert_eq!(out.key_source, p, "T → P in the key source");
        assert_eq!(
            out.value_template, value_template,
            "the value template (with its MappedValue placeholder) is untouched"
        );
        assert!(out.homomorphic);
    }

    /// M28: substitution rewrites mapped modifiers source and deferred-`keyof`
    /// operands as pure rewrites; evaluation still happens only at demand sites.
    #[test]
    fn substitute_rewrites_modifiers_source_and_keyof_operand() {
        use crate::types::repr::{MappedType, ModifierOp};

        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let t = interner.intern_type_param(TypeParamId(0), "T");
        let k = interner.intern_type_param(TypeParamId(1), "K");
        let placeholder = interner.intern_mapped_value();

        // `{ [P in K]: T[P] }` — the Pick shape: key source K, modifiers source T.
        let pick = interner.intern_mapped(MappedType {
            homomorphic: false,
            key_source: k,
            value_template: placeholder,
            modifiers_source: Some(t),
            optional_modifier: ModifierOp::Keep,
            readonly_modifier: ModifierOp::Keep,
        });
        let p = interner.intern_object(ObjectType {
            properties: vec![prop("a", wk.number)],
            ..Default::default()
        });
        let a_key = interner.intern_literal(crate::types::repr::LiteralValue::String("a".into()));
        let mut map = FxHashMap::default();
        map.insert(TypeParamId(0), p);
        map.insert(TypeParamId(1), a_key);

        let result = substitute(&mut interner, pick, &map);
        let out = interner
            .store()
            .mapped_type(result)
            .copied()
            .expect("result is a mapped type");
        assert_eq!(out.key_source, a_key, "K → \"a\" in the key source");
        assert_eq!(out.modifiers_source, Some(p), "T → P in the modifiers source");
        assert_eq!(
            out.value_template, placeholder,
            "the bound placeholder is never captured"
        );

        // `keyof T` → `keyof P`: a pure rewrite (no evaluation in substitution).
        let keyof_t = interner.intern_keyof(t);
        let rewritten = substitute(&mut interner, keyof_t, &map);
        assert_eq!(
            interner.store().keyof_operand(rewritten),
            Some(p),
            "the operand is rewritten and the node stays a deferred keyof"
        );
        // An unmapped operand leaves the node unchanged (no-op path).
        let u = interner.intern_type_param(TypeParamId(9), "U");
        let keyof_u = interner.intern_keyof(u);
        assert_eq!(substitute(&mut interner, keyof_u, &map), keyof_u);
    }

    /// M26 mapped distribution guard: naked-parameter union arguments distribute,
    /// but direct `keyof (A | B)` maps remain single nodes for common-key semantics.
    #[test]
    fn homomorphic_mapped_distributes_over_naked_param_union_only() {
        use crate::types::repr::{MappedType, ModifierOp};

        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let t = interner.intern_type_param(TypeParamId(0), "T");
        let placeholder = interner.intern_mapped_value();
        let a = interner.intern_object(ObjectType {
            properties: vec![prop("a", wk.number)],
            ..Default::default()
        });
        let b = interner.intern_object(ObjectType {
            properties: vec![prop("b", wk.string)],
            ..Default::default()
        });
        let ab = interner.union(vec![a, b]);

        // `Ident<T> = { [K in keyof T]: T[K] }`.
        let ident = interner.intern_mapped(MappedType {
            homomorphic: true,
            key_source: t,
            value_template: placeholder,
            modifiers_source: None,
            optional_modifier: ModifierOp::Keep,
            readonly_modifier: ModifierOp::Keep,
        });

        // Naked-param union argument distributes: Ident<A | B> = Ident<A> | Ident<B>.
        let mut map = FxHashMap::default();
        map.insert(TypeParamId(0), ab);
        let distributed = substitute(&mut interner, ident, &map);
        let ident_a = interner.intern_mapped(MappedType {
            homomorphic: true,
            key_source: a,
            value_template: placeholder,
            modifiers_source: None,
            optional_modifier: ModifierOp::Keep,
            readonly_modifier: ModifierOp::Keep,
        });
        let ident_b = interner.intern_mapped(MappedType {
            homomorphic: true,
            key_source: b,
            value_template: placeholder,
            modifiers_source: None,
            optional_modifier: ModifierOp::Keep,
            readonly_modifier: ModifierOp::Keep,
        });
        let expected = interner.union(vec![ident_a, ident_b]);
        assert_eq!(distributed, expected, "Ident<A | B> = Ident<A> | Ident<B>");

        // `never` distributes to zero members → never.
        let mut never_map = FxHashMap::default();
        never_map.insert(TypeParamId(0), wk.never);
        assert_eq!(
            substitute(&mut interner, ident, &never_map),
            wk.never,
            "Ident<never> = never"
        );

        // The DIRECT union form (key source already a union, not a parameter) must NOT
        // distribute: an unrelated substitution leaves it a single mapped node.
        let direct = interner.intern_mapped(MappedType {
            homomorphic: true,
            key_source: ab,
            value_template: placeholder,
            modifiers_source: None,
            optional_modifier: ModifierOp::Keep,
            readonly_modifier: ModifierOp::Keep,
        });
        let mut other_map = FxHashMap::default();
        other_map.insert(TypeParamId(9), wk.string);
        assert_eq!(
            substitute(&mut interner, direct, &other_map),
            direct,
            "the direct-union form stays a single mapped node (no distribution)"
        );
    }

    /// M27 — `substitute` rewrites a template's **holes** (its text segments untouched):
    /// `` `tag:${T}` `` → `` `tag:${string}` `` under `T → string`; a template with no
    /// mapped hole is returned unchanged (no-op path).
    #[test]
    fn substitution_rewrites_template_holes() {
        use crate::types::repr::TemplateType;
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let t = interner.intern_type_param(TypeParamId(0), "T");

        // `` `tag:${T}` `` and `` `x${U}` `` (U unmapped).
        let tag_t = interner.intern_template(TemplateType {
            texts: vec!["tag:".to_string(), String::new()],
            holes: vec![t],
        });
        let u = interner.intern_type_param(TypeParamId(1), "U");
        let x_u = interner.intern_template(TemplateType {
            texts: vec!["x".to_string(), String::new()],
            holes: vec![u],
        });

        let mut map = FxHashMap::default();
        map.insert(TypeParamId(0), wk.string);

        let tag_string = interner.intern_template(TemplateType {
            texts: vec!["tag:".to_string(), String::new()],
            holes: vec![wk.string],
        });
        assert_eq!(
            substitute(&mut interner, tag_t, &map),
            tag_string,
            "`tag:${{T}}` → `tag:${{string}}`"
        );
        assert_eq!(
            substitute(&mut interner, x_u, &map),
            x_u,
            "a template with no mapped hole is unchanged"
        );
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
                ..Default::default()
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

//! The per-`TypeTag` `apply_*` recursion arms of [`Substitution`] (dispatched from
//! `mod.rs`'s `apply`).

use super::*;
use crate::types::repr::{
    ConditionalType, FunctionType, MappedType, ObjectType, PropertyType, TemplateType, TupleType,
};

impl<'a> Substitution<'a> {
    /// Substitute through an object, re-interning only when a child changed.
    /// No-op substitution preserves nominal identity; re-entry returns the original
    /// id to break self-referential nominal cycles.
    pub(super) fn apply_object(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
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
                let new_write_ty = p.write_ty.map(|write_ty| {
                    let rewritten = self.apply(interner, write_ty);
                    changed |= rewritten != write_ty;
                    rewritten
                });
                PropertyType {
                    ty: new_ty,
                    write_ty: new_write_ty,
                    ..p
                }
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

    /// Substitute through a function type while preserving its own binders.
    /// Method-local binders shadow the incoming outer map, but their constraints
    /// and defaults still rewrite free outer references.
    pub(super) fn apply_function(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
        let Some(function) = interner.store().function_type(ty) else {
            return ty;
        };
        let type_params = function.type_params.clone();
        let receiver = function.receiver;
        let params = function.params.clone();
        let ret = function.ret;

        self.in_progress.insert(ty);
        let newly_blocked: Vec<TypeParamId> = type_params
            .iter()
            .filter_map(|param| self.blocked.insert(param.id).then_some(param.id))
            .collect();
        let mut changed = false;
        let type_params = type_params
            .into_iter()
            .map(|mut param| {
                let old_constraint = param.constraint;
                let old_default = param.default;
                param.constraint =
                    old_constraint.map(|constraint| self.apply(interner, constraint));
                param.default = old_default.map(|default| self.apply(interner, default));
                changed |= param.constraint != old_constraint || param.default != old_default;
                param
            })
            .collect();
        let lowered = params
            .into_iter()
            .map(|mut param| {
                let old_ty = param.ty;
                let new_ty = self.apply(interner, old_ty);
                changed |= new_ty != old_ty;
                param.ty = new_ty;
                param
            })
            .collect();
        let receiver = receiver.map(|receiver| {
            let new_receiver = self.apply(interner, receiver);
            changed |= new_receiver != receiver;
            new_receiver
        });
        let new_ret = self.apply(interner, ret);
        changed |= new_ret != ret;
        for id in newly_blocked {
            self.blocked.remove(&id);
        }
        self.in_progress.remove(&ty);

        if changed {
            interner.intern_function(FunctionType {
                type_params,
                receiver,
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
    pub(super) fn apply_union(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
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
    pub(super) fn apply_intersection(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
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
    pub(super) fn apply_array(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
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
    pub(super) fn apply_tuple(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
        let Some(tuple) = interner.store().tuple_type(ty) else {
            return ty;
        };
        let tuple = tuple.clone();

        let mut changed = false;
        let elements = tuple
            .elements
            .iter()
            .map(|&e| {
                let new_e = self.apply(interner, e);
                changed |= new_e != e;
                new_e
            })
            .collect();
        let rest = tuple.rest.map(|rest| {
            let new_ty = self.apply(interner, rest.ty);
            changed |= new_ty != rest.ty;
            crate::types::repr::TupleRestType { ty: new_ty, ..rest }
        });

        if changed {
            interner.intern_tuple_type(TupleType { elements, rest })
        } else {
            ty
        }
    }

    pub(super) fn apply_readonly(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
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
    pub(super) fn apply_conditional(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
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
    pub(super) fn apply_instantiation(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
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

    /// Compose an outer substitution into a declaration recipe's mapper without
    /// walking or materializing the recipe body.
    pub(super) fn apply_declared(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
        let Some(declared) = interner.store().declared_type(ty).cloned() else {
            return ty;
        };
        let free_params = interner
            .store()
            .declared_recipe(declared.recipe)
            .expect("declared application recipe exists")
            .free_params
            .clone();
        let existing: FxHashMap<_, _> = declared.mapper.iter().copied().collect();
        let mut changed = false;
        let mut mapper = Vec::new();
        for parameter in free_params {
            if let Some(value) = existing.get(&parameter).copied() {
                let substituted = self.apply(interner, value);
                changed |= substituted != value;
                mapper.push((parameter, substituted));
                continue;
            }
            if self.blocked.contains(&parameter) {
                continue;
            }
            if let Some(value) = self.map.get(&parameter).copied() {
                changed = true;
                mapper.push((parameter, value));
            }
        }
        if changed {
            interner.intern_declared(declared.recipe, mapper)
        } else {
            ty
        }
    }

    /// Substitute class arguments in declaration order without interpreting the
    /// application as an alias instantiation.
    pub(super) fn apply_class_instance(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
        let Some(instance) = interner.store().class_instance_type(ty) else {
            return ty;
        };
        let class = instance.class;
        let args = instance.args.clone();
        let mut changed = false;
        let args = args
            .into_iter()
            .map(|arg| {
                let substituted = self.apply(interner, arg);
                changed |= substituted != arg;
                substituted
            })
            .collect();
        if changed {
            interner.intern_class_instance(class, args)
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
    pub(super) fn apply_mapped(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
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
                            match substitute_with_outcome(interner, ty, &member_map) {
                                SubstitutionOutcome::CycleClean(result) => result,
                                SubstitutionOutcome::CycleTainted(result) => {
                                    self.cycle_epoch += 1;
                                    result
                                }
                            }
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
    pub(super) fn apply_keyof(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
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

    /// Substitute both ordered operands of a deferred indexed access.
    pub(super) fn apply_deferred_indexed_access(
        &mut self,
        interner: &mut Interner,
        ty: TypeId,
    ) -> TypeId {
        let Some(access) = interner.store().deferred_indexed_access_type(ty).copied() else {
            return ty;
        };
        let object = self.apply(interner, access.object);
        let index = self.apply(interner, access.index);
        if object == access.object && index == access.index {
            ty
        } else {
            interner.intern_deferred_indexed_access(object, index)
        }
    }

    /// Substitute template holes and re-intern only when a hole changed. Text
    /// segments are untouched; literal/union construction remains evaluator work.
    pub(super) fn apply_template(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
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

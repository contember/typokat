use super::*;

impl<'a> ConditionalEvaluator<'a> {
    /// Schedule the evaluation of a lazy instantiation `substitute(base, args)`. When
    /// `base` is a **distributive** conditional and the check argument distributes
    /// (union / `never` / `boolean`), build one concrete per-member conditional and union
    /// their results; otherwise a single concrete conditional.
    pub(super) fn eval_instantiation(
        &mut self,
        ty: TypeId,
        tasks: &mut Vec<Task>,
        values: &mut Vec<TypeId>,
        error: TypeId,
    ) {
        let Some(inst) = self.interner.store().instantiation_type(ty).cloned() else {
            values.push(ty);
            return;
        };
        if self.in_flight.contains(&ty) {
            self.note_cycle();
            values.push(error);
            return;
        }
        if self.exhausted {
            values.push(error);
            return;
        }

        // M28/WU3 — a **string-intrinsic** instantiation (`Uppercase<…>`), intercepted
        // by the prelude-declared marker identity: evaluate the argument through the
        // shared work-stack, then transform ([`Task::ApplyStringIntrinsic`]).
        let wk = self.interner.well_known();
        if wk.is_string_intrinsic_marker(inst.base) {
            let Some(&(_, arg)) = inst.args.first() else {
                // A marker with no argument (ill-formed `Uppercase` bare reference
                // routed here defensively) stays symbolic.
                values.push(ty);
                return;
            };
            self.steps += 1;
            if self.steps > self.budget {
                self.exhausted = true;
                values.push(error);
                return;
            }
            self.in_flight.insert(ty);
            tasks.push(Task::SetMemo(ty));
            tasks.push(Task::ApplyStringIntrinsic(ty));
            tasks.push(Task::Eval(arg));
            return;
        }

        self.in_flight.insert(ty);

        // M28/WU2 — a distributive conditional whose check argument is itself a
        // pending computation (`Exclude<keyof P, K>`: the check arg is a deferred-now-
        // concrete `keyof P`) must evaluate that argument FIRST — distribution derives
        // the per-member branches from the argument's VALUE (its union members), not
        // from the unevaluated node. Scheduled through the shared work-stack
        // ([`Task::ExpandDistributive`]); the M26 no-permissive-fallback rules are
        // untouched (a still-unevaluable argument distributes as a single member and
        // the branch test stays conservative).
        if let Some(arg) = self.distributive_check_arg(&inst) {
            if self.arg_needs_pre_eval(arg) {
                tasks.push(Task::SetMemo(ty));
                tasks.push(Task::ExpandDistributive(ty));
                tasks.push(Task::Eval(arg));
                return;
            }
        }

        let per_conditionals = self.expand_instantiation(&inst);

        tasks.push(Task::SetMemo(ty));
        tasks.push(Task::BuildUnion(per_conditionals.len()));
        // Push in reverse so they are evaluated (popped) in order — union is
        // order-independent, but this keeps behaviour deterministic.
        for &cond in per_conditionals.iter().rev() {
            tasks.push(Task::Eval(cond));
        }
    }

    /// The check argument of a distributive-conditional instantiation (M28): the value
    /// mapped to the base's naked check parameter, or `None` when the base is not a
    /// (non-poisoned) distributive conditional or the parameter is not in the args.
    pub(super) fn distributive_check_arg(
        &self,
        inst: &crate::types::repr::InstantiationType,
    ) -> Option<TypeId> {
        let check_param = self
            .interner
            .store()
            .conditional_type(inst.base)
            .filter(|c| c.distributive && !c.poisoned)
            .and_then(|c| self.interner.store().type_param(c.check).map(|p| p.id))?;
        inst.args
            .iter()
            .find(|(p, _)| *p == check_param)
            .map(|&(_, v)| v)
    }

    /// Whether an instantiation argument is a pending type-level computation that must
    /// evaluate before distribution (M28): an evaluable node, or a union containing one.
    pub(super) fn arg_needs_pre_eval(&self, arg: TypeId) -> bool {
        let evaluable = |t: TypeId| {
            matches!(
                self.interner.store().tag(t),
                TypeTag::Conditional
                    | TypeTag::Instantiation
                    | TypeTag::Mapped
                    | TypeTag::Template
                    | TypeTag::Keyof
            )
        };
        if evaluable(arg) {
            return true;
        }
        self.interner
            .store()
            .union_members(arg)
            .is_some_and(|members| members.iter().any(|&m| evaluable(m)))
    }

    /// Finish a distributive instantiation after pre-evaluating its check
    /// argument: re-derive per-member conditionals from the evaluated value, then
    /// let the enclosing [`Task::SetMemo`] commit under the original id.
    pub(super) fn expand_distributive(
        &mut self,
        ty: TypeId,
        tasks: &mut Vec<Task>,
        values: &mut Vec<TypeId>,
    ) {
        let evaluated_arg = values.pop().unwrap_or(self.interner.well_known().error);
        let Some(inst) = self.interner.store().instantiation_type(ty).cloned() else {
            // Defensive: leave the node as its own (deferred) value; SetMemo pops it.
            values.push(ty);
            return;
        };
        // Rebuild the argument list with the check parameter bound to the evaluated
        // value, then expand exactly like the direct path.
        let mut modified = inst;
        if let Some(check_param) = self
            .interner
            .store()
            .conditional_type(modified.base)
            .filter(|c| c.distributive && !c.poisoned)
            .and_then(|c| self.interner.store().type_param(c.check).map(|p| p.id))
        {
            for (param, value) in &mut modified.args {
                if *param == check_param {
                    *value = evaluated_arg;
                }
            }
        }
        let per_conditionals = self.expand_instantiation(&modified);
        tasks.push(Task::BuildUnion(per_conditionals.len()));
        for &cond in per_conditionals.iter().rev() {
            tasks.push(Task::Eval(cond));
        }
    }

    /// Produce the concrete conditional(s) an instantiation expands to. A distributive
    /// base whose check argument is a union / `never` / `boolean` yields one plain
    /// substitution per distributed member; anything else yields a single plain
    /// substitution.
    pub(super) fn expand_instantiation(
        &mut self,
        inst: &crate::types::repr::InstantiationType,
    ) -> Vec<TypeId> {
        // Is the base a distributive conditional whose check parameter is in the args?
        let check_param = self
            .interner
            .store()
            .conditional_type(inst.base)
            .filter(|c| c.distributive && !c.poisoned)
            .and_then(|c| self.interner.store().type_param(c.check).map(|p| p.id));

        if let Some(cp) = check_param {
            if let Some(&mapped) = inst.args.iter().find(|(p, _)| *p == cp).map(|(_, v)| v) {
                let members = self.distribute_members(mapped);
                return members
                    .into_iter()
                    .map(|m| self.substitute_member(inst, cp, m))
                    .collect();
            }
        }
        // Non-distributive (or check param absent): a single plain substitution.
        let map: FxHashMap<TypeParamId, TypeId> = inst.args.iter().copied().collect();
        vec![substitute(self.interner, inst.base, &map)]
    }

    /// Plain-substitute `inst.base` with `inst.args` but the check parameter `cp`
    /// overridden to a single distributed member `m` (so the branches are derived
    /// per-member, not with the whole union baked in). Never distributes further (`m` is
    /// a single type).
    pub(super) fn substitute_member(
        &mut self,
        inst: &crate::types::repr::InstantiationType,
        cp: TypeParamId,
        m: TypeId,
    ) -> TypeId {
        let mut map: FxHashMap<TypeParamId, TypeId> = inst.args.iter().copied().collect();
        map.insert(cp, m);
        substitute(self.interner, inst.base, &map)
    }

    /// The distribution members of a check argument: a union's members, with `never`
    /// members dropped (so `never` → no members → `never`) and the `boolean` intrinsic
    /// expanded to `true | false`. A non-union, non-`never`, non-`boolean` type is its
    /// own single member (evaluated once).
    pub(super) fn distribute_members(&mut self, ty: TypeId) -> Vec<TypeId> {
        let wk = self.interner.well_known();
        let raw: Vec<TypeId> = match self.interner.store().union_members(ty) {
            Some(members) => members.to_vec(),
            None => vec![ty],
        };
        let mut out: Vec<TypeId> = Vec::with_capacity(raw.len());
        for m in raw {
            if m == wk.never {
                // never contributes no members (distributes away).
            } else if m == wk.boolean {
                out.push(self.interner.intern_literal(LiteralValue::Boolean(true)));
                out.push(self.interner.intern_literal(LiteralValue::Boolean(false)));
            } else {
                out.push(m);
            }
        }
        out
    }

    /// Schedule evaluation of a union's members (re-unioning the results) — the shape a
    /// distributive conditional collapses to before its members are individually
    /// evaluated. A union with no conditional/instantiation member is already a value.
    pub(super) fn eval_union(
        &mut self,
        ty: TypeId,
        tasks: &mut Vec<Task>,
        values: &mut Vec<TypeId>,
    ) {
        let Some(members) = self.interner.store().union_members(ty) else {
            values.push(ty);
            return;
        };
        let members: Vec<TypeId> = members.to_vec();
        let needs_eval = members.iter().any(|&m| {
            matches!(
                self.interner.store().tag(m),
                TypeTag::Conditional
                    | TypeTag::Instantiation
                    | TypeTag::Mapped
                    | TypeTag::Template
                    | TypeTag::Keyof
            )
        });
        if !needs_eval {
            values.push(ty);
            return;
        }
        tasks.push(Task::BuildUnion(members.len()));
        for &m in members.iter().rev() {
            tasks.push(Task::Eval(m));
        }
    }

    /// Replace each `infer` binder (de Bruijn index `i`) with `fresh[i]`, recursing
    /// through structural composites and instantiation argument values, but **not** into
    /// a nested conditional (which rebinds its own indices — M25 does not model nested
    /// `infer`). Re-interns only when something changed.
    pub(super) fn substitute_infers(&mut self, ty: TypeId, fresh: &[TypeId]) -> TypeId {
        match self.interner.store().tag(ty) {
            TypeTag::Infer => match self.interner.store().infer_index(ty) {
                Some(i) => fresh.get(i as usize).copied().unwrap_or(ty),
                None => ty,
            },
            // A nested conditional rebinds its own infer indices; a mapped type / mapped
            // value placeholder (M26) carries no conditional infer binder — all opaque.
            TypeTag::Intrinsic
            | TypeTag::Literal
            | TypeTag::TypeParam
            | TypeTag::Conditional
            | TypeTag::Mapped
            | TypeTag::MappedValue => ty,
            TypeTag::Object => {
                let Some(object) = self.interner.store().object_type(ty).cloned() else {
                    return ty;
                };
                let mut changed = false;
                let mut new = object.clone();
                for prop in &mut new.properties {
                    let nt = self.substitute_infers(prop.ty, fresh);
                    changed |= nt != prop.ty;
                    prop.ty = nt;
                }
                new.string_index = object.string_index.map(|v| {
                    let nv = self.substitute_infers(v, fresh);
                    changed |= nv != v;
                    nv
                });
                new.number_index = object.number_index.map(|v| {
                    let nv = self.substitute_infers(v, fresh);
                    changed |= nv != v;
                    nv
                });
                new.call_signatures = object
                    .call_signatures
                    .iter()
                    .map(|&s| {
                        let ns = self.substitute_infers(s, fresh);
                        changed |= ns != s;
                        ns
                    })
                    .collect();
                new.construct_signatures = object
                    .construct_signatures
                    .iter()
                    .map(|&s| {
                        let ns = self.substitute_infers(s, fresh);
                        changed |= ns != s;
                        ns
                    })
                    .collect();
                if changed {
                    self.interner.intern_object(new)
                } else {
                    ty
                }
            }
            TypeTag::Function => {
                let Some(function) = self.interner.store().function_type(ty).cloned() else {
                    return ty;
                };
                let mut changed = false;
                let mut new = function.clone();
                for param in &mut new.params {
                    let nt = self.substitute_infers(param.ty, fresh);
                    changed |= nt != param.ty;
                    param.ty = nt;
                }
                let nr = self.substitute_infers(function.ret, fresh);
                changed |= nr != function.ret;
                new.ret = nr;
                if changed {
                    self.interner.intern_function(new)
                } else {
                    ty
                }
            }
            TypeTag::Union => {
                let Some(members) = self.interner.store().union_members(ty) else {
                    return ty;
                };
                let members: Vec<TypeId> = members.to_vec();
                let mut changed = false;
                let subst: Vec<TypeId> = members
                    .iter()
                    .map(|&m| {
                        let nm = self.substitute_infers(m, fresh);
                        changed |= nm != m;
                        nm
                    })
                    .collect();
                if changed {
                    self.interner.union(subst)
                } else {
                    ty
                }
            }
            // M31: freshen infer binders inside intersection members like a union.
            TypeTag::Intersection => {
                let Some(members) = self.interner.store().intersection_members(ty) else {
                    return ty;
                };
                let members: Vec<TypeId> = members.to_vec();
                let mut changed = false;
                let subst: Vec<TypeId> = members
                    .iter()
                    .map(|&m| {
                        let nm = self.substitute_infers(m, fresh);
                        changed |= nm != m;
                        nm
                    })
                    .collect();
                if changed {
                    self.interner.intersection(subst)
                } else {
                    ty
                }
            }
            TypeTag::Array => {
                let Some(element) = self.interner.store().array_type(ty).map(|a| a.element) else {
                    return ty;
                };
                let ne = self.substitute_infers(element, fresh);
                if ne != element {
                    self.interner.intern_array(ne)
                } else {
                    ty
                }
            }
            TypeTag::Tuple => {
                let Some(tuple) = self.interner.store().tuple_type(ty).cloned() else {
                    return ty;
                };
                let mut changed = false;
                let elements = tuple
                    .elements
                    .iter()
                    .map(|&e| {
                        let ne = self.substitute_infers(e, fresh);
                        changed |= ne != e;
                        ne
                    })
                    .collect();
                let rest = tuple.rest.map(|rest| {
                    let nt = self.substitute_infers(rest.ty, fresh);
                    changed |= nt != rest.ty;
                    TupleRestType { ty: nt, ..rest }
                });
                if changed {
                    self.interner
                        .intern_tuple_type(TupleType { elements, rest })
                } else {
                    ty
                }
            }
            TypeTag::Readonly => {
                let Some(operand) = self.interner.store().readonly_operand(ty) else {
                    return ty;
                };
                let no = self.substitute_infers(operand, fresh);
                if no != operand {
                    self.interner.intern_readonly(no)
                } else {
                    ty
                }
            }
            TypeTag::Instantiation => {
                let Some(inst) = self.interner.store().instantiation_type(ty).cloned() else {
                    return ty;
                };
                let mut changed = false;
                let new_args: Vec<(TypeParamId, TypeId)> = inst
                    .args
                    .iter()
                    .map(|&(p, v)| {
                        let nv = self.substitute_infers(v, fresh);
                        changed |= nv != v;
                        (p, nv)
                    })
                    .collect();
                if changed {
                    self.interner.intern_instantiation(inst.base, new_args)
                } else {
                    ty
                }
            }
            // M27: a template's infer binders live in its holes (`` `a${infer R}` `` in an
            // extends position) — freshen them by rewriting each hole.
            TypeTag::Template => {
                let Some(template) = self.interner.store().template_type(ty).cloned() else {
                    return ty;
                };
                let mut changed = false;
                let new_holes: Vec<TypeId> = template
                    .holes
                    .iter()
                    .map(|&hole| {
                        let nh = self.substitute_infers(hole, fresh);
                        changed |= nh != hole;
                        nh
                    })
                    .collect();
                if changed {
                    self.interner.intern_template(TemplateType {
                        texts: template.texts,
                        holes: new_holes,
                    })
                } else {
                    ty
                }
            }
            // M28: a `keyof (infer U)`-style extends component carries the binder in
            // the keyof operand — freshen it there.
            TypeTag::Keyof => {
                let Some(operand) = self.interner.store().keyof_operand(ty) else {
                    return ty;
                };
                let no = self.substitute_infers(operand, fresh);
                if no != operand {
                    self.interner.intern_keyof(no)
                } else {
                    ty
                }
            }
        }
    }
}

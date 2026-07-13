use super::*;

pub(in crate::check) struct InferenceConstraintEvaluation {
    pub(in crate::check) result: TypeId,
    pub(in crate::check) exhausted: bool,
    /// A constraint evaluator has no diagnostic span; retain the written constraint
    /// rather than accepting a cycle-derived error type.
    pub(in crate::check) cycle_detected: bool,
}

/// Demand-evaluate a constraint while fixing inferred type arguments. This mirrors
/// [`Pass::evaluate_type`] without a diagnostic span; callers use `exhausted` to avoid
/// accepting the budget-exhaustion error type as a real constraint value.
pub(in crate::check) fn evaluate_inference_constraint(
    interner: &mut Interner,
    next_type_param: &mut u32,
    ty: TypeId,
) -> InferenceConstraintEvaluation {
    let mut memo = FxHashMap::default();
    let mut ev = InferenceConstraintEvaluator::new(interner, next_type_param, &mut memo);
    let result = ev.evaluate(ty);
    InferenceConstraintEvaluation {
        result,
        exhausted: ev.exhausted,
        cycle_detected: ev.cycle_detected,
    }
}

struct InferenceConstraintEvaluator<'a> {
    interner: &'a mut Interner,
    next_type_param: &'a mut u32,
    memo: &'a mut FxHashMap<TypeId, TypeId>,
    in_progress: FxHashSet<TypeId>,
    exhausted: bool,
    cycle_detected: bool,
}

impl<'a> InferenceConstraintEvaluator<'a> {
    fn new(
        interner: &'a mut Interner,
        next_type_param: &'a mut u32,
        memo: &'a mut FxHashMap<TypeId, TypeId>,
    ) -> Self {
        InferenceConstraintEvaluator {
            interner,
            next_type_param,
            memo,
            in_progress: FxHashSet::default(),
            exhausted: false,
            cycle_detected: false,
        }
    }

    fn evaluate(&mut self, ty: TypeId) -> TypeId {
        if self.exhausted || self.in_progress.contains(&ty) {
            return ty;
        }
        match self.interner.store().tag(ty) {
            TypeTag::Conditional
            | TypeTag::Instantiation
            | TypeTag::Mapped
            | TypeTag::Template
            | TypeTag::Keyof => self.evaluate_pending(ty),
            TypeTag::Object => self.evaluate_object(ty),
            TypeTag::Function => self.evaluate_function(ty),
            TypeTag::Union => self.evaluate_union(ty),
            TypeTag::Intersection => self.evaluate_intersection(ty),
            TypeTag::Array => self.evaluate_array(ty),
            TypeTag::Tuple => self.evaluate_tuple(ty),
            TypeTag::Readonly => self.evaluate_readonly(ty),
            TypeTag::Intrinsic
            | TypeTag::Literal
            | TypeTag::TypeParam
            | TypeTag::Infer
            | TypeTag::MappedValue => ty,
        }
    }

    fn evaluate_pending(&mut self, ty: TypeId) -> TypeId {
        let result;
        let exhausted;
        let cycle_detected;
        {
            let mut ev = ConditionalEvaluator::new(
                self.interner,
                self.next_type_param,
                self.memo,
                DEFAULT_STEP_BUDGET,
            );
            result = ev.evaluate(ty);
            exhausted = ev.exhausted;
            cycle_detected = ev.cycle_detected;
        }
        if exhausted || cycle_detected {
            self.exhausted = true;
            self.cycle_detected |= cycle_detected;
            return ty;
        }
        if result == ty {
            ty
        } else {
            self.evaluate(result)
        }
    }

    fn evaluate_object(&mut self, ty: TypeId) -> TypeId {
        let Some(object) = self.interner.store().object_type(ty).cloned() else {
            return ty;
        };
        self.in_progress.insert(ty);
        let mut changed = false;
        let properties: Vec<PropertyType> = object
            .properties
            .into_iter()
            .map(|prop| {
                let new_ty = self.evaluate(prop.ty);
                changed |= new_ty != prop.ty;
                PropertyType { ty: new_ty, ..prop }
            })
            .collect();
        let string_index = object.string_index.map(|index_ty| {
            let new_ty = self.evaluate(index_ty);
            changed |= new_ty != index_ty;
            new_ty
        });
        let number_index = object.number_index.map(|index_ty| {
            let new_ty = self.evaluate(index_ty);
            changed |= new_ty != index_ty;
            new_ty
        });
        let call_signatures: Vec<TypeId> = object
            .call_signatures
            .into_iter()
            .map(|signature| {
                let new_ty = self.evaluate(signature);
                changed |= new_ty != signature;
                new_ty
            })
            .collect();
        let construct_signatures: Vec<TypeId> = object
            .construct_signatures
            .into_iter()
            .map(|signature| {
                let new_ty = self.evaluate(signature);
                changed |= new_ty != signature;
                new_ty
            })
            .collect();
        self.in_progress.remove(&ty);

        if changed {
            self.interner.intern_object(ObjectType {
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

    fn evaluate_function(&mut self, ty: TypeId) -> TypeId {
        let Some(function) = self.interner.store().function_type(ty).cloned() else {
            return ty;
        };
        self.in_progress.insert(ty);
        let mut changed = false;
        let receiver = function.receiver.map(|receiver| {
            let new_receiver = self.evaluate(receiver);
            changed |= new_receiver != receiver;
            new_receiver
        });
        let params: Vec<ParameterType> = function
            .params
            .into_iter()
            .map(|param| {
                let new_ty = self.evaluate(param.ty);
                changed |= new_ty != param.ty;
                ParameterType {
                    ty: new_ty,
                    ..param
                }
            })
            .collect();
        let ret = self.evaluate(function.ret);
        changed |= ret != function.ret;
        self.in_progress.remove(&ty);

        if changed {
            self.interner.intern_function(FunctionType {
                type_params: Vec::new(),
                receiver,
                params,
                ret,
            })
        } else {
            ty
        }
    }

    fn evaluate_union(&mut self, ty: TypeId) -> TypeId {
        let Some(members) = self.interner.store().union_members(ty).map(|m| m.to_vec()) else {
            return ty;
        };
        self.in_progress.insert(ty);
        let mut changed = false;
        let members: Vec<TypeId> = members
            .into_iter()
            .map(|member| {
                let new_ty = self.evaluate(member);
                changed |= new_ty != member;
                new_ty
            })
            .collect();
        self.in_progress.remove(&ty);

        if changed {
            self.interner.union(members)
        } else {
            ty
        }
    }

    fn evaluate_intersection(&mut self, ty: TypeId) -> TypeId {
        let Some(members) = self
            .interner
            .store()
            .intersection_members(ty)
            .map(|m| m.to_vec())
        else {
            return ty;
        };
        self.in_progress.insert(ty);
        let mut changed = false;
        let members: Vec<TypeId> = members
            .into_iter()
            .map(|member| {
                let new_ty = self.evaluate(member);
                changed |= new_ty != member;
                new_ty
            })
            .collect();
        self.in_progress.remove(&ty);

        if changed {
            self.interner.intersection(members)
        } else {
            ty
        }
    }

    fn evaluate_array(&mut self, ty: TypeId) -> TypeId {
        let Some(element) = self
            .interner
            .store()
            .array_type(ty)
            .map(|array| array.element)
        else {
            return ty;
        };
        let element = self.evaluate(element);
        if self
            .interner
            .store()
            .array_type(ty)
            .map(|array| array.element)
            == Some(element)
        {
            ty
        } else {
            self.interner.intern_array(element)
        }
    }

    fn evaluate_tuple(&mut self, ty: TypeId) -> TypeId {
        let Some(tuple) = self.interner.store().tuple_type(ty).cloned() else {
            return ty;
        };
        self.in_progress.insert(ty);
        let mut changed = false;
        let elements: Vec<TypeId> = tuple
            .elements
            .into_iter()
            .map(|element| {
                let new_ty = self.evaluate(element);
                changed |= new_ty != element;
                new_ty
            })
            .collect();
        let rest = tuple.rest.map(|rest| {
            let new_ty = self.evaluate(rest.ty);
            changed |= new_ty != rest.ty;
            TupleRestType { ty: new_ty, ..rest }
        });
        self.in_progress.remove(&ty);

        if changed {
            self.interner
                .intern_tuple_type(TupleType { elements, rest })
        } else {
            ty
        }
    }

    fn evaluate_readonly(&mut self, ty: TypeId) -> TypeId {
        let Some(operand) = self.interner.store().readonly_operand(ty) else {
            return ty;
        };
        let new_operand = self.evaluate(operand);
        if new_operand == operand {
            ty
        } else {
            self.interner.intern_readonly(new_operand)
        }
    }
}

impl<'a> ConditionalEvaluator<'a> {
    /// Run the `extends` test for a concrete conditional, returning
    /// `(matched, true_branch_with_infers_substituted)`. With `infer` binders present,
    /// their node-scoped de Bruijn indices are freshened to transient type parameters,
    /// candidates are collected through [`infer_from_types_for_conditional`] (same-name occurrences union
    /// — the covariant rule), and the matched candidates are substituted into both the
    /// `extends` type (before the relation test) and the true branch.
    pub(super) fn run_extends_test(&mut self, cond: &ConditionalType) -> (bool, TypeId) {
        if cond.infer_count == 0 {
            let matched = self.is_assignable(cond.check, cond.extends_ty);
            return (matched, cond.true_branch);
        }

        // Freshen the node's infer binders (de Bruijn 0..infer_count) to transient type
        // parameters, so the shared inference machinery can key candidates on them.
        let fresh: Vec<TypeId> = (0..cond.infer_count)
            .map(|_| {
                let id = TypeParamId(*self.next_type_param);
                *self.next_type_param += 1;
                self.interner.intern_type_param(id, "infer")
            })
            .collect();
        let fresh_ids: Vec<TypeParamId> = fresh
            .iter()
            .map(|&t| {
                self.interner
                    .store()
                    .type_param(t)
                    .map(|p| p.id)
                    .unwrap_or(TypeParamId(0))
            })
            .collect();

        let extends_f = self.substitute_infers(cond.extends_ty, &fresh);
        let true_f = self.substitute_infers(cond.true_branch, &fresh);

        // Collect candidates by matching the (concrete) check against the freshened
        // extends type — in CONDITIONAL mode: literals never widen (tsc keeps `"x"`;
        // widening would even corrupt a contravariant-position extends test) and a union
        // extends target descends into its members. Then fix each binder to the union of
        // its candidates (or `unknown` — the no-candidate fallback, matching tsc for an
        // infer left unbound in a taken branch).
        let mut candidates = crate::check::infer::Candidates::default();
        infer_from_types_for_conditional(self.interner, cond.check, extends_f, &mut candidates);
        let wk = self.interner.well_known();
        let mut map: FxHashMap<TypeParamId, TypeId> = FxHashMap::default();
        for &id in &fresh_ids {
            let fixed = match candidates.remove(&id) {
                Some(cands) if !cands.is_empty() => self.interner.union(cands),
                _ => wk.unknown,
            };
            map.insert(id, fixed);
        }

        let extends_final = substitute(self.interner, extends_f, &map);
        let matched = self.is_assignable(cond.check, extends_final);
        let true_final = substitute(self.interner, true_f, &map);
        (matched, true_final)
    }

    /// `src <: tgt` through the existing relation engine (a fresh [`Relater`] per test —
    /// the cache/cycle-stack invariants are untouched; the store is borrowed read-only
    /// while the relater lives, then released before the next interning step).
    pub(super) fn is_assignable(&self, src: TypeId, tgt: TypeId) -> bool {
        let wk = self.interner.well_known();
        let store = self.interner.store();
        let mut relater = Relater::new(store, wk);
        relater.is_assignable(src, tgt).is_yes()
    }

    /// Whether `ty` contains **no** free declaration type parameter (so a conditional
    /// whose check is `ty` may be evaluated). Iterative with a cache + cycle guard, so a
    /// 10 000-deep check type is classified without native-stack recursion and repeated
    /// sub-terms are `O(1)`.
    pub(super) fn is_concrete(&mut self, ty: TypeId) -> bool {
        if let Some(&c) = self.concrete.get(&ty) {
            return c;
        }
        // Iterative post-order: visit a node, push it back marked, then push its
        // children; on the second visit combine the (now cached) children.
        let mut stack: Vec<(TypeId, bool)> = vec![(ty, false)];
        let mut on_path: FxHashSet<TypeId> = FxHashSet::default();
        while let Some((t, expanded)) = stack.pop() {
            if self.concrete.contains_key(&t) {
                continue;
            }
            if !expanded {
                match self.interner.store().tag(t) {
                    TypeTag::TypeParam => {
                        self.concrete.insert(t, false);
                        continue;
                    }
                    // M26: a mapped-value placeholder is a bound variable — concrete. A
                    // mapped type falls through to descend into its key source + value
                    // template (below).
                    TypeTag::Intrinsic
                    | TypeTag::Literal
                    | TypeTag::Infer
                    | TypeTag::MappedValue => {
                        self.concrete.insert(t, true);
                        continue;
                    }
                    _ => {}
                }
                // A cycle back to a node already on the path: treat the back edge as
                // concrete (a free parameter, if any, is reached via a non-cycle path).
                if on_path.contains(&t) {
                    self.concrete.insert(t, true);
                    continue;
                }
                on_path.insert(t);
                stack.push((t, true));
                for child in self.child_types(t) {
                    if !self.concrete.contains_key(&child) {
                        stack.push((child, false));
                    }
                }
            } else {
                on_path.remove(&t);
                let concrete = self
                    .child_types(t)
                    .into_iter()
                    .all(|c| self.concrete.get(&c).copied().unwrap_or(true));
                self.concrete.insert(t, concrete);
            }
        }
        self.concrete.get(&ty).copied().unwrap_or(true)
    }

    /// The child type ids that determine a composite's concreteness. An
    /// instantiation's free parameters are those of its **argument values** (the base's
    /// own parameters are bound by the args), so the base is not a child here.
    pub(super) fn child_types(&self, ty: TypeId) -> Vec<TypeId> {
        let store = self.interner.store();
        match store.tag(ty) {
            TypeTag::Object => {
                let Some(object) = store.object_type(ty) else {
                    return Vec::new();
                };
                let mut out: Vec<TypeId> = object.properties.iter().map(|p| p.ty).collect();
                out.extend(object.string_index);
                out.extend(object.number_index);
                out.extend(object.call_signatures.iter().copied());
                out.extend(object.construct_signatures.iter().copied());
                out
            }
            TypeTag::Function => match store.function_type(ty) {
                Some(f) => {
                    let mut out: Vec<TypeId> = f.params.iter().map(|p| p.ty).collect();
                    out.extend(f.receiver);
                    out.push(f.ret);
                    out
                }
                None => Vec::new(),
            },
            TypeTag::Union => store
                .union_members(ty)
                .map(|m| m.to_vec())
                .unwrap_or_default(),
            // M31: an intersection is concrete once every member is.
            TypeTag::Intersection => store
                .intersection_members(ty)
                .map(|m| m.to_vec())
                .unwrap_or_default(),
            TypeTag::Array => store
                .array_type(ty)
                .map(|a| vec![a.element])
                .unwrap_or_default(),
            TypeTag::Tuple => store
                .tuple_type(ty)
                .map(|t| {
                    let mut out = t.elements.clone();
                    if let Some(rest) = t.rest {
                        out.push(rest.ty);
                    }
                    out
                })
                .unwrap_or_default(),
            TypeTag::Readonly => store
                .readonly_operand(ty)
                .map(|operand| vec![operand])
                .unwrap_or_default(),
            TypeTag::Conditional => store
                .conditional_type(ty)
                .map(|c| vec![c.check, c.extends_ty, c.true_branch, c.false_branch])
                .unwrap_or_default(),
            TypeTag::Instantiation => store
                .instantiation_type(ty)
                .map(|i| i.args.iter().map(|(_, v)| *v).collect())
                .unwrap_or_default(),
            // M26: a mapped type is concrete once its key source, value template, and
            // (M28) modifiers source are (the value template's `MappedValue`
            // placeholder is a bound variable — classified concrete above).
            TypeTag::Mapped => store
                .mapped_type(ty)
                .map(|m| {
                    let mut out = vec![m.key_source, m.value_template];
                    out.extend(m.modifiers_source);
                    out
                })
                .unwrap_or_default(),
            // M27: a template is concrete once every hole is (a free type parameter hole,
            // e.g. `` `tag:${T}` ``, makes it a deferred node).
            TypeTag::Template => store
                .template_type(ty)
                .map(|t| t.holes.clone())
                .unwrap_or_default(),
            // M28: a deferred keyof is concrete once its operand is.
            TypeTag::Keyof => store
                .keyof_operand(ty)
                .map(|operand| vec![operand])
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }
}

use super::*;

impl<'a> ConditionalEvaluator<'a> {
    /// Run the `extends` test for a concrete conditional, returning
    /// `(matched, true_branch_with_infers_substituted)`. With `infer` binders present,
    /// their node-scoped de Bruijn indices are freshened to transient type parameters,
    /// candidates are collected through [`infer_from_types_for_conditional`] (same-name occurrences union
    /// — the covariant rule), and the matched candidates are substituted into both the
    /// `extends` type (before the relation test) and the true branch.
    pub(super) fn run_extends_test_with(
        &mut self,
        cond: &ConditionalType,
        normalization: &dyn RelationNormalization,
    ) -> DemandOutcome<(bool, TypeId)> {
        if cond.infer_count == 0 {
            return match self.is_assignable_with(cond.check, cond.extends_ty, normalization) {
                DemandOutcome::Ready(matched) => DemandOutcome::Ready((matched, cond.true_branch)),
                DemandOutcome::Exhausted(exhaustion) => DemandOutcome::Exhausted(exhaustion),
            };
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
        let matched = match self.is_assignable_with(cond.check, extends_final, normalization) {
            DemandOutcome::Ready(matched) => matched,
            DemandOutcome::Exhausted(exhaustion) => return DemandOutcome::Exhausted(exhaustion),
        };
        let true_final = substitute(self.interner, true_f, &map);
        DemandOutcome::Ready((matched, true_final))
    }

    fn is_assignable_with(
        &self,
        src: TypeId,
        tgt: TypeId,
        normalization: &dyn RelationNormalization,
    ) -> DemandOutcome<bool> {
        let mut relater = Relater::planned(
            self.interner.store(),
            self.interner.well_known(),
            RelationCache::new(),
            normalization,
        );
        match relater.is_assignable_outcome(src, tgt) {
            RelationOutcome::Yes => DemandOutcome::Ready(true),
            RelationOutcome::No(_) => DemandOutcome::Ready(false),
            RelationOutcome::Exhausted(exhaustion) => DemandOutcome::Exhausted(exhaustion),
        }
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
            TypeTag::ClassInstance => store
                .class_instance_type(ty)
                .map(|instance| instance.args.clone())
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
            TypeTag::DeferredIndexedAccess => store
                .deferred_indexed_access_type(ty)
                .map(|access| vec![access.object, access.index])
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }
}

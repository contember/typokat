use super::*;
use crate::types::repr::{FunctionType, PropertyType, TupleType, Visibility};

#[derive(Copy, Clone)]
struct ParamSlot {
    ty: TypeId,
    accepts_undefined: bool,
}

#[derive(Clone)]
struct ParamShape {
    prefix: Vec<ParamSlot>,
    variadic: Option<ParamSlot>,
    suffix: Vec<ParamSlot>,
    non_array_rest: bool,
}

impl ParamShape {
    fn min_len(&self) -> usize {
        // Reaching a trailing suffix consumes every preceding prefix position.
        if !self.suffix.is_empty() {
            return self.prefix.len().saturating_add(self.suffix.len());
        }
        self.prefix
            .iter()
            .filter(|slot| !slot.accepts_undefined)
            .count()
    }

    fn parameter_count(&self) -> usize {
        debug_assert!(self.variadic.is_some() || self.suffix.is_empty());
        // Moving suffixes stay inside the aggregate rest tail, not stable positions.
        self.prefix.len() + usize::from(self.variadic.is_some())
    }

    fn has_non_array_rest(&self) -> bool {
        self.non_array_rest
    }

    fn positional_slot(&self, index: usize) -> Option<ParamSlot> {
        self.prefix.get(index).copied().or(self.variadic)
    }

    fn tail(&self, start: usize) -> ParamTail<'_> {
        ParamTail {
            prefix: self.prefix.get(start..).unwrap_or_default(),
            variadic: self.variadic,
            suffix: &self.suffix,
        }
    }
}

#[derive(Copy, Clone)]
struct ParamTail<'a> {
    prefix: &'a [ParamSlot],
    variadic: Option<ParamSlot>,
    suffix: &'a [ParamSlot],
}

#[derive(Copy, Clone)]
struct ParamTailSlot {
    slot: ParamSlot,
    required: bool,
    rest: bool,
}

impl ParamTail<'_> {
    fn min_len(self) -> usize {
        if !self.suffix.is_empty() {
            return self.prefix.len().saturating_add(self.suffix.len());
        }
        self.prefix
            .iter()
            .filter(|slot| !slot.accepts_undefined)
            .count()
    }

    fn max_len(self) -> Option<usize> {
        if self.variadic.is_some() {
            None
        } else {
            Some(self.prefix.len() + self.suffix.len())
        }
    }

    fn slot_count(self) -> usize {
        self.prefix.len() + usize::from(self.variadic.is_some()) + self.suffix.len()
    }

    fn start_count(self) -> usize {
        self.prefix.len()
    }

    fn end_count(self) -> usize {
        self.suffix.len()
    }

    fn slot(self, index: usize) -> Option<ParamTailSlot> {
        if let Some(slot) = self.prefix.get(index).copied() {
            return Some(ParamTailSlot {
                slot,
                required: !self.suffix.is_empty() || !slot.accepts_undefined,
                rest: false,
            });
        }
        let rest_index = self.prefix.len();
        if let Some(slot) = self.variadic {
            if index == rest_index {
                return Some(ParamTailSlot {
                    slot,
                    required: false,
                    rest: true,
                });
            }
        }
        let suffix_index = index.checked_sub(rest_index + usize::from(self.variadic.is_some()))?;
        self.suffix
            .get(suffix_index)
            .copied()
            .map(|slot| ParamTailSlot {
                slot,
                required: true,
                rest: false,
            })
    }
}

impl<'a> Relater<'a> {
    /// Property-wise object relation. Returns the **first** failing target
    /// property (in canonical order) as a precise reason: a `MissingProperty`
    /// when the source lacks a required target property, or a `Property` wrapping
    /// the nested reason when the property is present but its type does not
    /// relate. First-failure ordering keeps the verdict deterministic and matches
    /// how the M2 corpus pairs one cause per failing assignment.
    ///
    /// M19 — **index signatures**: after the named-property obligations, if the
    /// **target** has a string index signature, every **named property of the
    /// source** (any key is a string key) and the source's own string index value
    /// (if any) must be assignable to the target's string index value type; likewise
    /// a target number index signature constrains the source's **numeric-named**
    /// properties and the source's number index value. A single failing value is
    /// reported as one `IndexSignature` reason (`TK2322`). The target's index
    /// signatures are checked here; the source having *extra* index signatures the
    /// target lacks is fine (width).
    pub(super) fn relate_objects(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut AssumedSet,
    ) -> Relation {
        // Both ids are object-tagged here; the side-tables always resolve. The
        // `else` arms are defensive (an object tag without a payload is a store
        // invariant violation, never expected) and produce a leaf rather than
        // panicking. These borrows are of `*self.store` (lifetime `'a`), independent
        // of `&mut self`, so they live across the recursive `self.relate` calls.
        let Some(tgt_obj) = self.store.object_type(tgt) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let Some(src_obj) = self.store.object_type(src) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };

        // Both `intern_object` and `fill_object` sort properties by name. Advance the
        // source cursor monotonically while retaining a same-name match so malformed
        // duplicate target names still observe the first matching source property.
        let mut source_cursor = 0;
        let mut previous_target_name: Option<&str> = None;
        let mut previous_source_property: Option<&PropertyType> = None;
        for tgt_prop in &tgt_obj.properties {
            #[cfg(test)]
            super::measure_relation(|measure| measure.object_target_properties += 1);
            let source_property = if previous_target_name == Some(tgt_prop.name.as_str()) {
                previous_source_property
            } else {
                let found = loop {
                    let Some(src_prop) = src_obj.properties.get(source_cursor) else {
                        break None;
                    };
                    #[cfg(test)]
                    super::measure_relation(|measure| {
                        measure.object_source_property_comparisons += 1
                    });
                    match src_prop.name.cmp(&tgt_prop.name) {
                        std::cmp::Ordering::Less => source_cursor += 1,
                        std::cmp::Ordering::Equal => {
                            source_cursor += 1;
                            break Some(src_prop);
                        }
                        std::cmp::Ordering::Greater => break None,
                    }
                };
                previous_target_name = Some(tgt_prop.name.as_str());
                previous_source_property = found;
                found
            };
            match source_property {
                // Width + depth: present in source — its type must relate to the
                // target property's type (recurse, so nested mismatches nest).
                Some(src_prop) => {
                    // M13 — nominal rule: a `private`/`protected` **target** member
                    // requires the source's same-named member to share its *origin*
                    // (same visibility AND same declaring class). This is what makes
                    // a class with a non-public member nominal: a structurally equal
                    // object literal (public, no origin) or another class's
                    // same-named non-public member does NOT satisfy it, so the
                    // relation FAILS here rather than reporting a depth mismatch.
                    // Soundness-critical: a non-matching origin must make the
                    // relation fail (no false negative). A public target member
                    // imposes no origin requirement (pure structural), so M0–M12
                    // objects are unaffected.
                    //
                    // The failure is a plain object-level `Leaf` (the member's *type*
                    // is fine — only its visibility/origin differs), so it maps to
                    // `TK2322` with no misleading "number is not assignable to
                    // number" depth elaboration. Object-target messages are asserted
                    // code-only in the corpus, so the headline form is unconstrained.
                    if !nominal_origin_ok(tgt_prop, src_prop) {
                        return Relation::No(ReasonChain::leaf(src, tgt));
                    }
                    // M21: presence is independent of the value type. An *optional* source
                    // property may be ABSENT, so it cannot satisfy a *required* target
                    // property (which must be present) — regardless of whether their value
                    // types relate (e.g. a required target of `string | undefined` still
                    // rejects an optional source, because the source may OMIT it entirely).
                    // required->optional and both-optional are fine; the value relation
                    // below handles the rest. Like the nominal rule above, this is an
                    // object-level structural failure, so it is a plain `Leaf` (not a
                    // value-depth `Property` — the member's *type* may relate fine).
                    if src_prop.optional && !tgt_prop.optional {
                        return Relation::No(ReasonChain::leaf(src, tgt));
                    }
                    if let Relation::No(child) =
                        self.relate(src_prop.ty, tgt_prop.ty, kind, assumed)
                    {
                        return Relation::No(ReasonChain::of(Reason::Property {
                            name: tgt_prop.name.clone(),
                            src,
                            tgt,
                            because: Box::new(child.head),
                        }));
                    }
                }
                // Target property absent in the source.
                None => {
                    // M21: an *optional* target property may be absent — skip it. Its
                    // effective type already includes `undefined` (unioned in at
                    // lowering), so a *present* source value is related normally in the
                    // `Some(..)` arm above; absence is simply allowed. Only a *required*
                    // absent target property is a missing-property failure (`TK2741`).
                    if tgt_prop.optional {
                        continue;
                    }
                    return Relation::No(ReasonChain::of(Reason::MissingProperty {
                        name: tgt_prop.name.clone(),
                        src,
                        tgt,
                    }));
                }
            }
        }

        // F1/WU2 — call-signature obligation. A target object with a call
        // signature requires the source object to have a compatible call
        // signature; an object without one cannot satisfy a callable target. This
        // obligation goes through `self.relate` on the interned FunctionType ids,
        // preserving the relation cache/cycle-stack invariants.
        if let Relation::No(child) = self.relate_object_call_signatures(src, tgt, kind, assumed) {
            return Relation::No(child);
        }

        // F1/WU3 — construct-signature obligation. Mirrors the call-signature
        // obligation above: a target object with `new (...)` requires the source
        // object to have a compatible single construct signature, compared by the
        // existing FunctionType relation through `self.relate`.
        if let Relation::No(child) =
            self.relate_object_construct_signatures(src, tgt, kind, assumed)
        {
            return Relation::No(child);
        }

        // M19 — index-signature obligations. Snapshot the target's index value types
        // and the source's named-property/(index) value types up front so the
        // recursive `self.relate` below does not overlap the read borrows.
        let tgt_string_index = tgt_obj.string_index;
        let tgt_number_index = tgt_obj.number_index;
        if tgt_string_index.is_none() && tgt_number_index.is_none() {
            // No index signature on the target — nothing further to check (the M0–M18
            // structural verdict stands).
            return Relation::Yes;
        }
        // (name, value type) of each source named property, plus the source's own
        // index value types (the source may itself be an index-sig object — its index
        // value must fit the target's, like a dictionary assigned to a dictionary).
        let src_props: Vec<(String, TypeId)> = src_obj
            .properties
            .iter()
            .map(|p| (p.name.clone(), p.ty))
            .collect();
        let src_string_index = src_obj.string_index;
        let src_number_index = src_obj.number_index;

        // String index target: every source named property (any name is a string
        // key) AND the source's own string index value must be assignable to it.
        if let Some(tgt_value) = tgt_string_index {
            for (_, src_value) in &src_props {
                if let Relation::No(child) = self.relate(*src_value, tgt_value, kind, assumed) {
                    return Relation::No(ReasonChain::of(Reason::IndexSignature {
                        src,
                        tgt,
                        because: Box::new(child.head),
                    }));
                }
            }
            if let Some(src_value) = src_string_index {
                if let Relation::No(child) = self.relate(src_value, tgt_value, kind, assumed) {
                    return Relation::No(ReasonChain::of(Reason::IndexSignature {
                        src,
                        tgt,
                        because: Box::new(child.head),
                    }));
                }
            }
        }

        // Number index target: only the source's **numeric-named** properties are
        // constrained (a number index sig does not govern arbitrary string keys),
        // plus the source's own number index value.
        if let Some(tgt_value) = tgt_number_index {
            for (name, src_value) in &src_props {
                if !is_numeric_property_name(name) {
                    continue;
                }
                if let Relation::No(child) = self.relate(*src_value, tgt_value, kind, assumed) {
                    return Relation::No(ReasonChain::of(Reason::IndexSignature {
                        src,
                        tgt,
                        because: Box::new(child.head),
                    }));
                }
            }
            if let Some(src_value) = src_number_index {
                if let Relation::No(child) = self.relate(src_value, tgt_value, kind, assumed) {
                    return Relation::No(ReasonChain::of(Reason::IndexSignature {
                        src,
                        tgt,
                        because: Box::new(child.head),
                    }));
                }
            }
        }

        Relation::Yes
    }

    fn relate_object_call_signatures(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut AssumedSet,
    ) -> Relation {
        let Some(tgt_obj) = self.store.object_type(tgt) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        if tgt_obj.call_signatures.is_empty() {
            return Relation::Yes;
        }
        let Some(src_obj) = self.store.object_type(src) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let src_signatures = src_obj.call_signatures.clone();
        let tgt_signatures = tgt_obj.call_signatures.clone();
        self.relate_signature_sets(src, tgt, &src_signatures, &tgt_signatures, kind, assumed)
    }

    fn relate_object_construct_signatures(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut AssumedSet,
    ) -> Relation {
        let Some(tgt_obj) = self.store.object_type(tgt) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        if tgt_obj.construct_signatures.is_empty() {
            return Relation::Yes;
        }
        let Some(src_obj) = self.store.object_type(src) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let src_signatures = src_obj.construct_signatures.clone();
        let tgt_signatures = tgt_obj.construct_signatures.clone();
        self.relate_construct_signature_sets(
            src,
            tgt,
            &src_signatures,
            &tgt_signatures,
            kind,
            assumed,
        )
    }

    pub(super) fn relate_object_to_function(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut AssumedSet,
    ) -> Relation {
        let Some(src_obj) = self.store.object_type(src) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        if src_obj.call_signatures.is_empty() {
            return Relation::No(ReasonChain::leaf(src, tgt));
        }
        let src_signatures = src_obj.call_signatures.clone();
        for src_sig in src_signatures {
            if matches!(self.relate(src_sig, tgt, kind, assumed), Relation::Yes) {
                return Relation::Yes;
            }
        }
        Relation::No(ReasonChain::leaf(src, tgt))
    }

    pub(super) fn relate_function_to_object(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut AssumedSet,
    ) -> Relation {
        let Some(tgt_obj) = self.store.object_type(tgt) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        if tgt_obj.call_signatures.is_empty() {
            return Relation::No(ReasonChain::leaf(src, tgt));
        }
        // A plain function value has no represented named members. It can satisfy
        // optional target properties by omission, but any required target member is
        // missing and remains a `TK2741`-shaped relation failure.
        for tgt_prop in &tgt_obj.properties {
            if !tgt_prop.optional {
                return Relation::No(ReasonChain::of(Reason::MissingProperty {
                    name: tgt_prop.name.clone(),
                    src,
                    tgt,
                }));
            }
        }

        if !tgt_obj.construct_signatures.is_empty() {
            return Relation::No(ReasonChain::leaf(src, tgt));
        }
        let tgt_signatures = tgt_obj.call_signatures.clone();
        for tgt_sig in tgt_signatures {
            if let Relation::No(_) = self.relate(src, tgt_sig, kind, assumed) {
                return Relation::No(ReasonChain::leaf(src, tgt));
            }
        }
        Relation::Yes
    }

    fn relate_signature_sets(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        src_signatures: &[TypeId],
        tgt_signatures: &[TypeId],
        kind: RelationKind,
        assumed: &mut AssumedSet,
    ) -> Relation {
        if src_signatures.is_empty() || tgt_signatures.is_empty() {
            return Relation::No(ReasonChain::leaf(src, tgt));
        }
        for tgt_sig in tgt_signatures {
            let mut matched = false;
            for src_sig in src_signatures {
                if matches!(
                    self.relate(*src_sig, *tgt_sig, kind, assumed),
                    Relation::Yes
                ) {
                    matched = true;
                    break;
                }
            }
            if !matched {
                return Relation::No(ReasonChain::leaf(src, tgt));
            }
        }
        Relation::Yes
    }

    fn relate_construct_signature_sets(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        src_signatures: &[TypeId],
        tgt_signatures: &[TypeId],
        kind: RelationKind,
        assumed: &mut AssumedSet,
    ) -> Relation {
        if src_signatures.is_empty() || tgt_signatures.is_empty() {
            return Relation::No(ReasonChain::leaf(src, tgt));
        }
        for tgt_sig in tgt_signatures {
            let mut matched = false;
            for src_sig in src_signatures {
                if matches!(
                    self.relate_construct_functions(*src_sig, *tgt_sig, kind, assumed),
                    Relation::Yes
                ) {
                    matched = true;
                    break;
                }
            }
            if !matched && src_signatures.len() > 1 {
                for src_sig in src_signatures {
                    if matches!(
                        self.relate_overloaded_construct_to_generic_target(
                            *src_sig, *tgt_sig, kind, assumed
                        ),
                        Relation::Yes
                    ) {
                        matched = true;
                        break;
                    }
                }
            }
            if !matched {
                return Relation::No(ReasonChain::leaf(src, tgt));
            }
        }
        Relation::Yes
    }

    /// Construct signatures use the normal function relation except when their
    /// generic arities differ. In that case TypeScript compares the shared shape
    /// under source specialization while extra target binders stay constrained.
    fn relate_construct_functions(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut AssumedSet,
    ) -> Relation {
        let Some(src_fn) = self.store.function_type(src).cloned() else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let Some(tgt_fn) = self.store.function_type(tgt).cloned() else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        if src_fn.type_params.is_empty()
            || tgt_fn.type_params.is_empty()
            || src_fn.type_params.len() == tgt_fn.type_params.len()
        {
            return self.relate_functions(src, tgt, kind, assumed);
        }

        let Some(context) = BinderRelationContext::construct_arity_specialization(
            &src_fn.type_params,
            &tgt_fn.type_params,
        ) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let mut context = context;
        let shared = src_fn.type_params.len().min(tgt_fn.type_params.len());

        // An unconstrained return-only source binder can specialize to the target
        // return shape. A constrained return-only binder instead contributes its
        // apparent constraint; parameter occurrences retain specialization.
        for parameter in &src_fn.type_params[shared..] {
            if parameter.constraint.is_some()
                && !self.function_parameters_contain_type_param(&src_fn, parameter.id)
            {
                context.instantiable_source.remove(&parameter.id);
            }
        }

        // An unused target binder has no observable call shape, so it cannot make
        // an otherwise compatible constructor fail. A used target binder remains
        // universal; without an apparent constraint, accepting it would be a
        // false negative.
        for parameter in &tgt_fn.type_params[shared..] {
            if !self.function_shape_contains_type_param(&tgt_fn, parameter.id) {
                context.parameters.remove(&parameter.id);
                context.constraints.remove(&parameter.id);
            } else if parameter.constraint.is_none() {
                return Relation::No(ReasonChain::leaf(src, tgt));
            }
        }
        self.with_binder_context(context, |relater| {
            if !relater.generic_function_constraints_compatible(&src_fn, &tgt_fn, kind, assumed) {
                return Relation::No(ReasonChain::leaf(src, tgt));
            }
            relater.relate_function_shapes(src, tgt, &src_fn, &tgt_fn, kind, assumed)
        })
    }

    fn function_parameters_contain_type_param(
        &self,
        function: &FunctionType,
        parameter: TypeParamId,
    ) -> bool {
        function
            .receiver
            .is_some_and(|receiver| self.type_contains_type_param(receiver, parameter))
            || function
                .params
                .iter()
                .any(|slot| self.type_contains_type_param(slot.ty, parameter))
    }

    fn function_shape_contains_type_param(
        &self,
        function: &FunctionType,
        parameter: TypeParamId,
    ) -> bool {
        self.function_parameters_contain_type_param(function, parameter)
            || self.type_contains_type_param(function.ret, parameter)
    }

    /// Check a signature child structurally rather than treating a binder as
    /// observable merely because it was declared. The visited set makes recursive
    /// object and alias shapes safe to inspect.
    fn type_contains_type_param(&self, ty: TypeId, parameter: TypeParamId) -> bool {
        let mut pending = vec![ty];
        let mut seen = FxHashSet::default();

        while let Some(current) = pending.pop() {
            if !seen.insert(current) {
                continue;
            }
            match self.store.tag(current) {
                TypeTag::TypeParam => {
                    if self.store.type_param(current).map(|item| item.id) == Some(parameter) {
                        return true;
                    }
                }
                TypeTag::Object => {
                    if let Some(object) = self.store.object_type(current) {
                        pending.extend(object.properties.iter().map(|property| property.ty));
                        pending.extend(object.string_index);
                        pending.extend(object.number_index);
                        pending.extend(object.call_signatures.iter().copied());
                        pending.extend(object.construct_signatures.iter().copied());
                    }
                }
                TypeTag::Function => {
                    if let Some(nested) = self.store.function_type(current) {
                        pending.extend(nested.receiver);
                        pending.extend(nested.params.iter().map(|slot| slot.ty));
                        pending.push(nested.ret);
                        pending.extend(
                            nested
                                .type_params
                                .iter()
                                .flat_map(|binder| [binder.constraint, binder.default])
                                .flatten(),
                        );
                    }
                }
                TypeTag::Union => {
                    if let Some(members) = self.store.union_members(current) {
                        pending.extend(members.iter().copied());
                    }
                }
                TypeTag::Intersection => {
                    if let Some(members) = self.store.intersection_members(current) {
                        pending.extend(members.iter().copied());
                    }
                }
                TypeTag::Array => {
                    if let Some(array) = self.store.array_type(current) {
                        pending.push(array.element);
                    }
                }
                TypeTag::Tuple => {
                    if let Some(tuple) = self.store.tuple_type(current) {
                        pending.extend(tuple.elements.iter().copied());
                        pending.extend(tuple.rest.map(|rest| rest.ty));
                    }
                }
                TypeTag::Readonly => {
                    pending.extend(self.store.readonly_operand(current));
                }
                TypeTag::Conditional => {
                    if let Some(conditional) = self.store.conditional_type(current) {
                        pending.extend([
                            conditional.check,
                            conditional.extends_ty,
                            conditional.true_branch,
                            conditional.false_branch,
                        ]);
                    }
                }
                TypeTag::Instantiation => {
                    if let Some(instantiation) = self.store.instantiation_type(current) {
                        pending.push(instantiation.base);
                        pending.extend(instantiation.args.iter().map(|(_, argument)| *argument));
                    }
                }
                TypeTag::ClassInstance => {
                    if let Some(instance) = self.store.class_instance_type(current) {
                        pending.extend(instance.args.iter().copied());
                    }
                }
                TypeTag::Mapped => {
                    if let Some(mapped) = self.store.mapped_type(current) {
                        pending.extend(
                            [
                                Some(mapped.key_source),
                                Some(mapped.value_template),
                                mapped.modifiers_source,
                            ]
                            .into_iter()
                            .flatten(),
                        );
                    }
                }
                TypeTag::Template => {
                    if let Some(template) = self.store.template_type(current) {
                        pending.extend(template.holes.iter().copied());
                    }
                }
                TypeTag::Keyof => {
                    pending.extend(self.store.keyof_operand(current));
                }
                TypeTag::DeferredIndexedAccess => {
                    if let Some(access) = self.store.deferred_indexed_access_type(current) {
                        pending.push(access.object);
                        pending.push(access.index);
                    }
                }
                TypeTag::Intrinsic | TypeTag::Literal | TypeTag::Infer | TypeTag::MappedValue => {}
            }
        }

        false
    }

    fn relate_overloaded_construct_to_generic_target(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut AssumedSet,
    ) -> Relation {
        let Some(src_fn) = self.store.function_type(src).cloned() else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let Some(tgt_fn) = self.store.function_type(tgt).cloned() else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        if src_fn.type_params.is_empty() || src_fn.type_params.len() != tgt_fn.type_params.len() {
            return Relation::No(ReasonChain::leaf(src, tgt));
        }
        let context = BinderRelationContext::aligned(&src_fn.type_params, &tgt_fn.type_params);
        self.with_binder_context(context, |relater| {
            relater.relate_function_shapes(src, tgt, &src_fn, &tgt_fn, kind, assumed)
        })
    }

    /// Function assignability (mvp-plan §6.5, architecture §6.5). `src` is
    /// assignable to `tgt` iff:
    ///
    ///  - **arity is satisfiable** — fixed target shapes reject surplus required
    ///    source slots, while a target variadic absorbs compatible fixed slots, and
    ///  - each **parameter is contravariant** — the *target's* parameter type is
    ///    assignable to the *source's* parameter type (`tgt_param → src_param`);
    ///    source rests cover every remaining target prefix/rest/suffix slot, and
    ///  - the **return type is covariant** — the *source's* return type is
    ///    assignable to the *target's* return type (`src_ret → tgt_ret`), **except**
    ///    that a `void` target return accepts any source return type (a
    ///    value-returning function is assignable to a void-returning function
    ///    type; the value is discarded).
    ///
    /// typokat is contravariant on parameters everywhere (it picks soundness over
    /// tsc's method bivariance — methods are not in the MVP subset), matching tsc
    /// under `strictFunctionTypes` for function-typed values. The first failing
    /// obligation (arity, then parameters left-to-right, then return) is returned
    /// as a precise, nestable reason for M6.
    pub(super) fn relate_functions(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut AssumedSet,
    ) -> Relation {
        // Both ids are function-tagged here; the side-tables always resolve. The
        // `else` arms are defensive (a function tag without a payload is a store
        // invariant violation, never expected) and produce a leaf rather than
        // panicking.
        let Some(src_fn) = self.store.function_type(src).cloned() else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let Some(tgt_fn) = self.store.function_type(tgt).cloned() else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };

        match (src_fn.type_params.is_empty(), tgt_fn.type_params.is_empty()) {
            (true, true) => self.relate_function_shapes(src, tgt, &src_fn, &tgt_fn, kind, assumed),
            // A non-generic implementation cannot stand in for every instantiation
            // a generic target permits. Keep this rejection explicit rather than
            // falling through to the declaration-side apparent-type rule.
            (true, false) => Relation::No(ReasonChain::leaf(src, tgt)),
            // A generic source can be specialized for a concrete target. The local
            // context fixes a binder at its first compatible target occurrence and
            // checks the persistent descriptor constraint before accepting it.
            (false, true) => {
                let context = BinderRelationContext::source_specialization(&src_fn.type_params);
                self.with_binder_context(context, |relater| {
                    relater.relate_function_shapes(src, tgt, &src_fn, &tgt_fn, kind, assumed)
                })
            }
            (false, false) => {
                if src_fn.type_params.len() != tgt_fn.type_params.len() {
                    return Relation::No(ReasonChain::leaf(src, tgt));
                }
                let context =
                    BinderRelationContext::aligned(&src_fn.type_params, &tgt_fn.type_params);
                self.with_binder_context(context, |relater| {
                    if !relater
                        .generic_function_constraints_compatible(&src_fn, &tgt_fn, kind, assumed)
                    {
                        return Relation::No(ReasonChain::leaf(src, tgt));
                    }
                    relater.relate_function_shapes(src, tgt, &src_fn, &tgt_fn, kind, assumed)
                })
            }
        }
    }

    /// Compare two alpha-aligned generic constraints. A target generic can be
    /// instantiated anywhere inside its constraint, so its bound must be accepted
    /// by the source bound (`target constraint -> source constraint`). Defaults are
    /// intentionally call-site-only and never affect this relation.
    fn generic_function_constraints_compatible(
        &mut self,
        src_fn: &FunctionType,
        tgt_fn: &FunctionType,
        kind: RelationKind,
        assumed: &mut AssumedSet,
    ) -> bool {
        for (src_param, tgt_param) in src_fn.type_params.iter().zip(&tgt_fn.type_params) {
            let src_constraint = src_param.constraint.unwrap_or(self.well_known.unknown);
            let tgt_constraint = tgt_param.constraint.unwrap_or(self.well_known.unknown);
            if !self
                .relate(tgt_constraint, src_constraint, kind, assumed)
                .is_yes()
            {
                return false;
            }
        }
        true
    }

    pub(crate) fn overload_implementation_compatible_attempt(
        &mut self,
        overload_ty: TypeId,
        implementation_ty: TypeId,
    ) -> super::RelationAttempt {
        self.allow_relation_demand = true;
        self.query_exhaustion = None;
        self.query_demand = None;
        self.query_demand_observed = false;
        let compatible =
            self.overload_implementation_compatible_inner(overload_ty, implementation_ty);
        if let Some(exhaustion) = self.query_exhaustion.take() {
            return super::RelationAttempt::Decided(super::RelationOutcome::Exhausted(exhaustion));
        }
        if let Some(demand) = self.query_demand.take() {
            return super::RelationAttempt::Needs(demand);
        }
        if !compatible {
            return super::RelationAttempt::Decided(super::RelationOutcome::No(
                super::ReasonChain::leaf(overload_ty, implementation_ty),
            ));
        }
        super::RelationAttempt::Decided(super::RelationOutcome::Yes)
    }

    fn overload_implementation_compatible_inner(
        &mut self,
        overload_ty: TypeId,
        implementation_ty: TypeId,
    ) -> bool {
        let Some(overload) = self.store.function_type(overload_ty).cloned() else {
            return false;
        };
        let Some(implementation) = self.store.function_type(implementation_ty).cloned() else {
            return false;
        };

        match (
            overload.type_params.is_empty(),
            implementation.type_params.is_empty(),
        ) {
            // Preserve the existing non-generic overload rule: implementations may
            // return a representable supertype (for example `number | string` for
            // separate `number`/`string` overloads).
            (true, true) => self.overload_implementation_shapes(&overload, &implementation, false),
            // A generic implementation can specialize to one fixed overload. This
            // is ordinary function assignability in implementation -> overload
            // direction: overload arguments flow into the implementation and its
            // specialized result flows back out to the overload caller.
            (true, false) => self.nested_assignable(implementation_ty, overload_ty),
            (false, true) => {
                let context = BinderRelationContext::source_specialization(&overload.type_params);
                self.with_binder_context(context, |relater| {
                    relater.overload_implementation_shapes(&overload, &implementation, true)
                })
            }
            (false, false) => {
                if overload.type_params.len() != implementation.type_params.len() {
                    return false;
                }
                let context = BinderRelationContext::aligned(
                    &overload.type_params,
                    &implementation.type_params,
                );
                self.with_binder_context(context, |relater| {
                    for (overload_param, implementation_param) in
                        overload.type_params.iter().zip(&implementation.type_params)
                    {
                        let overload_constraint = overload_param
                            .constraint
                            .unwrap_or(relater.well_known.unknown);
                        let implementation_constraint = implementation_param
                            .constraint
                            .unwrap_or(relater.well_known.unknown);
                        if !relater
                            .nested_assignable(overload_constraint, implementation_constraint)
                        {
                            return false;
                        }
                    }
                    relater.overload_implementation_shapes(&overload, &implementation, true)
                })
            }
        }
    }

    fn overload_implementation_shapes(
        &mut self,
        overload: &FunctionType,
        implementation: &FunctionType,
        implementation_return_to_overload: bool,
    ) -> bool {
        if let (Some(overload_receiver), Some(implementation_receiver)) =
            (overload.receiver, implementation.receiver)
        {
            if !self.nested_assignable(overload_receiver, implementation_receiver) {
                return false;
            }
        }
        if implementation.required_param_count() > overload.required_param_count() {
            return false;
        }
        for (overload_param, implementation_param) in
            overload.params.iter().zip(&implementation.params)
        {
            if overload_param.rest != implementation_param.rest
                || !self.nested_assignable(overload_param.ty, implementation_param.ty)
            {
                return false;
            }
        }
        let overload_to_implementation = self.nested_assignable(overload.ret, implementation.ret);
        if !implementation_return_to_overload {
            return overload_to_implementation;
        }
        overload_to_implementation || self.nested_assignable(implementation.ret, overload.ret)
    }

    fn nested_assignable(&mut self, source: TypeId, target: TypeId) -> bool {
        let mut assumed = rustc_hash::FxHashSet::default();
        self.relate(source, target, RelationKind::Assignable, &mut assumed)
            .is_yes()
    }

    /// The context-free positional function rule, reused after a generic relation
    /// frame has aligned binders or temporarily specialized a generic source.
    fn relate_function_shapes(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        src_fn: &FunctionType,
        tgt_fn: &FunctionType,
        kind: RelationKind,
        assumed: &mut AssumedSet,
    ) -> Relation {
        if let (Some(src_receiver), Some(tgt_receiver)) = (src_fn.receiver, tgt_fn.receiver) {
            if matches!(
                self.relate(tgt_receiver, src_receiver, kind, assumed),
                Relation::No(_)
            ) {
                return Relation::No(ReasonChain::leaf(src, tgt));
            }
        }
        let Some(src_params) = self.function_param_shape(src_fn) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        let Some(tgt_params) = self.function_param_shape(tgt_fn) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };

        // A moving target suffix changes the rest rule: after the stable target
        // prefix, a variadic source may add only omittable fixed slots. A finite
        // source may add none because the target has no maximum length.
        let arity_fails = if tgt_params.variadic.is_some() {
            let suffix_exceeds_shortest = !src_params.suffix.is_empty()
                && src_params
                    .prefix
                    .len()
                    .saturating_add(src_params.suffix.len())
                    > tgt_params.min_len();
            if tgt_params.suffix.is_empty() {
                suffix_exceeds_shortest
            } else if src_params.variadic.is_some() {
                suffix_exceeds_shortest
                    || src_params
                        .prefix
                        .iter()
                        .skip(tgt_params.prefix.len())
                        .any(|slot| !slot.accepts_undefined)
            } else {
                src_params
                    .prefix
                    .len()
                    .saturating_add(src_params.suffix.len())
                    > tgt_params.prefix.len()
            }
        } else {
            src_params.min_len() > tgt_params.min_len()
        };
        if arity_fails {
            return Relation::No(ReasonChain::of(Reason::ParameterCount { src, tgt }));
        }

        let (src_ret, tgt_ret) = (src_fn.ret, tgt_fn.ret);

        if let Relation::No(child) =
            self.relate_function_parameters(src, tgt, &src_params, &tgt_params, kind, assumed)
        {
            return Relation::No(child);
        }

        // Return: COVARIANT — the source return must be assignable to the target
        // return (`src_ret → tgt_ret`). Exception: a target return type of `void`
        // accepts **any** source return type — a value-returning function is
        // assignable to a void-returning function type, since the extra value is
        // simply discarded by the caller (`() => 1` is assignable to
        // `() => void`). This is the standard TS rule for void-returning
        // function-typed values.
        if tgt_ret != self.well_known.void {
            if let Relation::No(child) = self.relate(src_ret, tgt_ret, kind, assumed) {
                return Relation::No(ReasonChain::of(Reason::ReturnType {
                    src,
                    tgt,
                    because: Box::new(child.head),
                }));
            }
        }

        Relation::Yes
    }

    fn relate_function_parameters(
        &mut self,
        src: TypeId,
        tgt: TypeId,
        src_params: &ParamShape,
        tgt_params: &ParamShape,
        kind: RelationKind,
        assumed: &mut AssumedSet,
    ) -> Relation {
        let aggregate_tail = src_params.has_non_array_rest() || tgt_params.has_non_array_rest();
        let src_count = src_params.parameter_count();
        let tgt_count = tgt_params.parameter_count();
        let parameter_count = if aggregate_tail {
            src_count.min(tgt_count)
        } else {
            src_count.max(tgt_count)
        };
        let stable_count = parameter_count.saturating_sub(usize::from(aggregate_tail));

        for index in 0..stable_count {
            #[cfg(test)]
            super::measure_relation(|measure| measure.function_parameter_positions += 1);
            let Some(src_slot) = src_params.positional_slot(index) else {
                continue;
            };
            let Some(tgt_slot) = tgt_params.positional_slot(index) else {
                continue;
            };
            if let Relation::No(child) =
                self.relate_parameter_slot(tgt_slot, src_slot, kind, assumed)
            {
                return Relation::No(ReasonChain::of(Reason::Parameter {
                    index,
                    src,
                    tgt,
                    because: Box::new(child.head),
                }));
            }
        }

        if aggregate_tail && parameter_count != 0 {
            let index = parameter_count - 1;
            let target_arguments = tgt_params.tail(index);
            let source_accepts = src_params.tail(index);
            if let Relation::No(child) = self.relate_parameter_tail(
                tgt,
                src,
                target_arguments,
                source_accepts,
                kind,
                assumed,
            ) {
                return Relation::No(ReasonChain::of(Reason::Parameter {
                    index,
                    src,
                    tgt,
                    because: Box::new(child.head),
                }));
            }
        }

        Relation::Yes
    }

    /// Compare the target signature's remaining argument tuple with the tail the
    /// source signature accepts. This is the aggregate non-array-rest position
    /// from TypeScript's signature relation, represented as borrowed shape views
    /// so relation stays read-only and allocation-free.
    fn relate_parameter_tail(
        &mut self,
        argument_root: TypeId,
        accepted_root: TypeId,
        arguments: ParamTail<'_>,
        accepts: ParamTail<'_>,
        kind: RelationKind,
        assumed: &mut AssumedSet,
    ) -> Relation {
        if arguments.min_len() < accepts.min_len()
            || accepts.max_len().is_some_and(|accepted_max| {
                arguments
                    .max_len()
                    .is_none_or(|argument_max| argument_max > accepted_max)
            })
        {
            return Relation::No(ReasonChain::leaf(argument_root, accepted_root));
        }

        let argument_count = arguments.slot_count();
        let accepted_count = accepts.slot_count();
        for argument_index in 0..argument_count {
            #[cfg(test)]
            super::measure_relation(|measure| measure.function_parameter_positions += 1);
            let Some(argument) = arguments.slot(argument_index) else {
                return Relation::No(ReasonChain::leaf(argument_root, accepted_root));
            };
            let argument_from_end = argument_count - 1 - argument_index;
            let accepted_index =
                if accepts.variadic.is_some() && argument_index >= accepts.start_count() {
                    accepted_count.checked_sub(1 + argument_from_end.min(accepts.end_count()))
                } else {
                    Some(argument_index)
                };
            let Some(accepted) = accepted_index.and_then(|index| accepts.slot(index)) else {
                return Relation::No(ReasonChain::leaf(argument_root, accepted_root));
            };
            if accepted.required && (argument.rest || !argument.required) {
                return Relation::No(ReasonChain::leaf(argument_root, accepted_root));
            }
            if let Relation::No(child) =
                self.relate_parameter_slot(argument.slot, accepted.slot, kind, assumed)
            {
                return Relation::No(child);
            }
        }
        Relation::Yes
    }

    fn relate_parameter_slot(
        &mut self,
        src_slot: ParamSlot,
        tgt_slot: ParamSlot,
        kind: RelationKind,
        assumed: &mut AssumedSet,
    ) -> Relation {
        if let Relation::No(child) = self.relate(src_slot.ty, tgt_slot.ty, kind, assumed) {
            if tgt_slot.accepts_undefined
                && self
                    .relate(src_slot.ty, self.well_known.undefined, kind, assumed)
                    .is_yes()
            {
                return Relation::Yes;
            }
            return Relation::No(child);
        }
        if src_slot.accepts_undefined && !tgt_slot.accepts_undefined {
            return self.relate(self.well_known.undefined, tgt_slot.ty, kind, assumed);
        }
        Relation::Yes
    }

    fn function_param_shape(&self, function: &FunctionType) -> Option<ParamShape> {
        let mut prefix: Vec<ParamSlot> = function
            .fixed_params()
            .map(|param| ParamSlot {
                ty: param.ty,
                accepts_undefined: param.optional || param.has_default,
            })
            .collect();
        let Some(rest) = function.rest_param() else {
            return Some(ParamShape {
                prefix,
                variadic: None,
                suffix: Vec::new(),
                non_array_rest: false,
            });
        };
        let rest_shape = self.rest_param_shape(rest.ty)?;
        prefix.extend(rest_shape.prefix);
        Some(ParamShape {
            prefix,
            variadic: rest_shape.variadic,
            suffix: rest_shape.suffix,
            non_array_rest: rest_shape.non_array_rest,
        })
    }

    fn rest_param_shape(&self, ty: TypeId) -> Option<ParamShape> {
        let ty = self.store.readonly_operand(ty).unwrap_or(ty);
        if let Some(array) = self.store.array_type(ty) {
            return Some(ParamShape {
                prefix: Vec::new(),
                variadic: Some(ParamSlot {
                    ty: array.element,
                    accepts_undefined: false,
                }),
                suffix: Vec::new(),
                non_array_rest: false,
            });
        }
        if let Some(tuple) = self.store.tuple_type(ty) {
            return self.tuple_param_shape(tuple);
        }
        Some(ParamShape {
            prefix: Vec::new(),
            variadic: Some(ParamSlot {
                ty,
                accepts_undefined: false,
            }),
            suffix: Vec::new(),
            non_array_rest: true,
        })
    }

    fn tuple_param_shape(&self, tuple: &TupleType) -> Option<ParamShape> {
        let Some(rest) = tuple.rest else {
            return Some(ParamShape {
                prefix: tuple
                    .elements
                    .iter()
                    .map(|&ty| ParamSlot {
                        ty,
                        accepts_undefined: false,
                    })
                    .collect(),
                variadic: None,
                suffix: Vec::new(),
                non_array_rest: false,
            });
        };
        if rest.position > tuple.elements.len() {
            return None;
        }
        let mut prefix: Vec<ParamSlot> = tuple.elements[..rest.position]
            .iter()
            .map(|&ty| ParamSlot {
                ty,
                accepts_undefined: false,
            })
            .collect();
        let suffix: Vec<ParamSlot> = tuple.elements[rest.position..]
            .iter()
            .map(|&ty| ParamSlot {
                ty,
                accepts_undefined: false,
            })
            .collect();
        let rest_shape = self.rest_param_shape(rest.ty)?;
        prefix.extend(rest_shape.prefix);
        let nested_non_array_rest = rest_shape.non_array_rest;
        let mut combined_suffix = rest_shape.suffix;
        combined_suffix.extend(suffix);
        if rest_shape.variadic.is_none() {
            prefix.extend(combined_suffix);
            return Some(ParamShape {
                prefix,
                variadic: None,
                suffix: Vec::new(),
                non_array_rest: false,
            });
        }
        // A prefix-only tuple rest collapses to its array tail; a suffix keeps it aggregate.
        let non_array_rest = nested_non_array_rest || !combined_suffix.is_empty();
        Some(ParamShape {
            prefix,
            variadic: rest_shape.variadic,
            suffix: combined_suffix,
            non_array_rest,
        })
    }

    /// Whether the **merged source** — the intersection of `cands` — relates to `tgt`
    /// (M31, the sound core of the source-intersection rule; no interning, as the
    /// relation engine holds the store read-only). Recursive, so the merge is decided at
    /// every depth:
    ///
    ///  - **object `tgt`** (no index / call / construct signature): the merged source
    ///    must satisfy **every** target property — an AND that sees ALL contributed
    ///    properties, never a some-member shortcut. For a property named by exactly ONE
    ///    member the merged value IS that member's type, so it delegates to the
    ///    cycle-guarded main engine (`self.relate`, exact); a property named by SEVERAL
    ///    members has a genuine merged (intersection) value, decided by **recursing** with
    ///    those `subcands` — NOT a some-candidate shortcut (which would reintroduce the
    ///    finding-1 hole one level down). An uncovered required property is a
    ///    `MissingProperty` (`TK2741`); a covered-but-incompatible one a `Property`
    ///    (`TK2322`). This threads the shared `assumed` (an AND — all contributions
    ///    matter);
    ///  - an object `tgt` carrying an **index / call / construct signature** is out of
    ///    the merged subset → conservative `No` (a documented safe over-report; it is
    ///    also a genuine rejection when a member's value violates the target's index);
    ///  - **non-object `tgt`** (primitive / function / tuple / array / …): no presence
    ///    penalty, so a **single** candidate assignable to `tgt` suffices (sound). An OR,
    ///    so — like [`Relater::relate_union_target`] — each attempt uses its own local
    ///    accumulator and only the winner's assumptions merge up (a union target was
    ///    already decomposed by the earlier target-union dispatch, so `tgt` here is never
    ///    a union).
    ///
    /// **Termination (§6.3-sound).** The multi-contributor per-property recursion can
    /// re-enter the same `(merged source, target)` on a recursive object property
    /// (`interface P { p: P }`; `P & Q <: P`). `in_flight` is an **assume-true** cycle
    /// guard, exactly the coinductive rule the main cycle stack uses
    /// (`relation.rs` §"Cycle stack FIRST"): re-entering a `(sorted cands, tgt, kind)`
    /// already in flight returns `Yes`. The cycle is self-contained within one
    /// `relate_intersection_source` query and discharged at its root (each object-target
    /// entry removes its key on exit); this method **never** touches the durable cache,
    /// and single-contributor properties flow through `self.relate` (so genuine
    /// cross-relation cycles still bubble through `assumed`) — so no provisional `Yes` is
    /// ever durably cached.
    pub(super) fn relate_source_members_to(
        &mut self,
        src: TypeId,
        cands: &[TypeId],
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut AssumedSet,
        in_flight: &mut MergedInFlightSet,
    ) -> Relation {
        if self.store.tag(tgt) == TypeTag::Object {
            // Assume-true cycle guard for the merged-source recursion (see the doc above).
            // The key canonicalizes the candidate set (sorted) so `[P, Q]` and `[Q, P]`
            // collide.
            let mut key_cands = cands.to_vec();
            key_cands.sort_unstable();
            let key = (key_cands, tgt, kind);
            if in_flight.contains(&key) {
                return Relation::Yes;
            }
            in_flight.insert(key.clone());
            let result =
                self.relate_merged_object_properties(src, cands, tgt, kind, assumed, in_flight);
            in_flight.remove(&key);
            return result;
        }

        // Non-object target: no presence penalty, so a single assignable candidate
        // suffices (OR — per-attempt accumulator, merge only the winner).
        let mut last_child: Option<ReasonChain> = None;
        for &cand in cands {
            let mut cand_assumed: AssumedSet = FxHashSet::default();
            match self.relate(cand, tgt, kind, &mut cand_assumed) {
                Relation::Yes => {
                    assumed.extend(cand_assumed);
                    return Relation::Yes;
                }
                Relation::No(child) => last_child = Some(child),
            }
        }
        Relation::No(last_child.unwrap_or_else(|| ReasonChain::leaf(src, tgt)))
    }

    /// The per-property body of the object-target branch of [`Relater::relate_source_members_to`]
    /// (extracted so the caller can bracket it with the `in_flight` insert/remove). The
    /// merged source (`cands`) must satisfy **every** target property.
    fn relate_merged_object_properties(
        &mut self,
        src: TypeId,
        cands: &[TypeId],
        tgt: TypeId,
        kind: RelationKind,
        assumed: &mut AssumedSet,
        in_flight: &mut MergedInFlightSet,
    ) -> Relation {
        let Some(tgt_obj) = self.store.object_type(tgt) else {
            return Relation::No(ReasonChain::leaf(src, tgt));
        };
        if tgt_obj.string_index.is_some()
            || tgt_obj.number_index.is_some()
            || !tgt_obj.call_signatures.is_empty()
            || !tgt_obj.construct_signatures.is_empty()
        {
            return Relation::No(ReasonChain::leaf(src, tgt));
        }

        // Snapshot (target property + contributing source properties) per target property
        // BEFORE any recursive relate, so no store borrow is held across it. The full
        // contributor `PropertyType` is carried (not just `.ty`) so the nominal-origin
        // rule (M13) can be enforced on the merged path exactly as on the normal object
        // path — otherwise an intersection source would satisfy a private/protected target
        // member structurally (an unsound accept). The value-relation list is still keyed
        // by DISTINCT type (sorted + deduped), so a property named by one type takes the
        // single-contributor path even if two members agree on it.
        struct Obligation {
            tgt_prop: PropertyType,
            contributors: Vec<PropertyType>,
        }
        let obligations: Vec<Obligation> = tgt_obj
            .properties
            .iter()
            .map(|tgt_prop| {
                let contributors: Vec<PropertyType> = cands
                    .iter()
                    .filter_map(|&m| {
                        self.store
                            .object_type(m)
                            .and_then(|o| o.property(&tgt_prop.name))
                            .cloned()
                    })
                    .collect();
                Obligation {
                    tgt_prop: tgt_prop.clone(),
                    contributors,
                }
            })
            .collect();

        for ob in obligations {
            let name = ob.tgt_prop.name.clone();
            if ob.contributors.is_empty() {
                // An optional target property may be absent from the merged source;
                // a required one is genuinely missing.
                if ob.tgt_prop.optional {
                    continue;
                }
                return Relation::No(ReasonChain::of(Reason::MissingProperty { name, src, tgt }));
            }
            // M13 nominal rule on the merged path: a non-public target member requires at
            // least one contributing source member to share its origin (same visibility +
            // declaring class). A purely structural intersection (public members, no
            // origin) therefore cannot satisfy a private/protected target — the same
            // deterministic, assumption-free leaf failure the normal path reports. Checked
            // before any recursive relate, so it never depends on an in-flight assumption
            // and cannot make the cached verdict query-order dependent.
            if !ob
                .contributors
                .iter()
                .any(|src_prop| nominal_origin_ok(&ob.tgt_prop, src_prop))
            {
                return Relation::No(ReasonChain::leaf(src, tgt));
            }
            let mut subcands: Vec<TypeId> = ob.contributors.iter().map(|p| p.ty).collect();
            subcands.sort_unstable();
            subcands.dedup();
            let tgt_ty = ob.tgt_prop.ty;
            // Part A — a single DISTINCT contributor: the merged value IS that type, so
            // delegate to the cycle-guarded main engine (exact `(c) <: tgt_ty` ≡
            // `relate(c, tgt_ty)`; reuses the cache + cycle stack, threads `assumed`).
            // Part B — several contributors: a genuine merged value, recursed through the
            // in-flight-guarded merged engine. Both thread the shared `assumed` (an AND).
            let child = if let [only] = subcands.as_slice() {
                self.relate(*only, tgt_ty, kind, assumed)
            } else {
                self.relate_source_members_to(src, &subcands, tgt_ty, kind, assumed, in_flight)
            };
            if let Relation::No(child) = child {
                return Relation::No(ReasonChain::of(Reason::Property {
                    name,
                    src,
                    tgt,
                    because: Box::new(child.head),
                }));
            }
        }
        Relation::Yes
    }
}

/// Whether a property name is numeric-keyed and thus governed by a number index
/// signature. The name must parse as a finite `f64`; ordinary identifiers remain
/// pure string keys.
fn is_numeric_property_name(name: &str) -> bool {
    name.parse::<f64>().map(|n| n.is_finite()).unwrap_or(false)
}

/// Whether the source member satisfies the target member's nominal-origin
/// requirement (M13). Non-public target members require the same visibility and
/// declaring class; a failed check must make the relation fail.
fn nominal_origin_ok(tgt_prop: &PropertyType, src_prop: &PropertyType) -> bool {
    match tgt_prop.visibility {
        // Public target member: structural only — no origin constraint.
        Visibility::Public => true,
        // Non-public target member: the source member must share its exact origin
        // (same visibility and same declaring class). A class's own instances pass
        // trivially (the identity fast path in `relate` short-circuits same-id
        // types before this is ever reached); everything else must match here.
        Visibility::Private | Visibility::Protected => {
            src_prop.visibility == tgt_prop.visibility
                && src_prop.declaring_class == tgt_prop.declaring_class
        }
    }
}

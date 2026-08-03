use super::helpers::{function_shape, property_pairs, segment_matches_hole};
use super::*;
use crate::types::repr::{
    FunctionType, LiteralValue, ModifierOp, TemplateType, TupleRestType, TupleType,
};

/// The per-inference recursion state: the cycle guard for structural matching, plus
/// the **collection mode** (M25). Built once per entry-point call and dropped after.
pub(super) struct InferenceContext<'query> {
    /// `(source, target)` id pairs currently on the recursion stack. Re-entering a
    /// pair short-circuits (it is already contributing candidates further up). See
    /// the module docs.
    visited: FxHashSet<(TypeId, TypeId)>,
    /// Descend into a union **target**'s members with a non-union source (M25,
    /// review MEDIUM-3): `number[]` against `string | (infer U)[]` collects `U` from the
    /// array member (same-name contributions union). Call-site inference keeps only the
    /// M10 equal-length pairwise union rule (no behavior change for m10).
    union_target_descent: bool,
    normalization: Option<&'query dyn crate::relate::RelationNormalization>,
    exhaustion: Option<crate::class_semantics::Exhaustion>,
}

impl<'query> InferenceContext<'query> {
    /// Conditional-`infer` mode (M25): a union extends target collects from its
    /// members; a template-pattern target captures `infer` holes (M27).
    pub(super) fn for_conditional() -> Self {
        InferenceContext {
            visited: FxHashSet::default(),
            union_target_descent: true,
            normalization: None,
            exhaustion: None,
        }
    }

    pub(super) fn for_query(
        union_target_descent: bool,
        normalization: &'query dyn crate::relate::RelationNormalization,
    ) -> Self {
        InferenceContext {
            visited: FxHashSet::default(),
            union_target_descent,
            normalization: Some(normalization),
            exhaustion: None,
        }
    }

    pub(super) fn take_exhaustion(&mut self) -> Option<crate::class_semantics::Exhaustion> {
        self.exhaustion.take()
    }

    /// Match `source` against `target`, recording candidates. The dispatch on the
    /// target's tag is what decides whether a type parameter is being inferred
    /// (target is a `TypeParam`) or whether to recurse structurally.
    pub(super) fn infer(
        &mut self,
        interner: &mut Interner,
        source: TypeId,
        target: TypeId,
        candidates: &mut Candidates,
    ) {
        if self.exhaustion.is_some() {
            return;
        }
        let (source, target) = if let Some(normalization) = self.normalization {
            let source = match normalization.normalize(source) {
                Ok(source) => source,
                Err(reason) => {
                    self.exhaustion = Some(reason);
                    return;
                }
            };
            let target = match normalization.normalize(target) {
                Ok(target) => target,
                Err(reason) => {
                    self.exhaustion = Some(reason);
                    return;
                }
            };
            (source, target)
        } else {
            (source, target)
        };
        // A target type parameter is the one place a candidate is recorded — always
        // AS-IS (raw). Call-site widening happens at fix time (per
        // parameter); conditional-`infer` extraction never widens (tsc keeps `"x"` —
        // M25 review HIGH-1/HIGH-2).
        if interner.store().tag(target) == TypeTag::TypeParam {
            if let Some(param) = interner.store().type_param(target) {
                let id = param.id;
                candidates.entry(id).or_default().push(source);
            }
            return;
        }

        // Structural recursion is cycle-guarded: re-entering an in-flight
        // (source, target) pair adds nothing, so short-circuit.
        if !self.visited.insert((source, target)) {
            return;
        }

        // Call-site inference treats `null`/`undefined` as non-inference members when a
        // nullable union has exactly one substantive target. Contextual argument walking
        // uses the same target, so fresh literals retain their structural candidates.
        if !self.union_target_descent && interner.store().tag(target) == TypeTag::Union {
            let substantive = interner
                .store()
                .union_members(target)
                .into_iter()
                .flatten()
                .copied()
                .filter(|member| {
                    !matches!(
                        interner.store().intrinsic_kind(*member),
                        Some(IntrinsicKind::Null | IntrinsicKind::Undefined)
                    )
                })
                .collect::<Vec<_>>();
            if let [member] = substantive.as_slice() {
                self.infer(interner, source, *member, candidates);
                self.visited.remove(&(source, target));
                return;
            }
        }

        // M25 conditional mode: a union target descends into members. Naked infer
        // members are low-priority whole-check candidates and are dropped when a
        // structural member of THIS union bound the same binder.
        if self.union_target_descent
            && interner.store().tag(target) == TypeTag::Union
            && interner.store().tag(source) != TypeTag::Union
        {
            let members: Vec<TypeId> = interner
                .store()
                .union_members(target)
                .map(|m| m.to_vec())
                .unwrap_or_default();
            let (naked, structural): (Vec<TypeId>, Vec<TypeId>) = members
                .into_iter()
                .partition(|&m| interner.store().tag(m) == TypeTag::TypeParam);
            let mut structural_cands = Candidates::default();
            for member in structural {
                self.infer(interner, source, member, &mut structural_cands);
            }
            for member in naked {
                let bound_structurally = interner
                    .store()
                    .type_param(member)
                    .is_some_and(|p| structural_cands.get(&p.id).is_some_and(|c| !c.is_empty()));
                if bound_structurally {
                    continue;
                }
                // Kept: the ordinary TypeParam-target arm records it (mode-gated widen).
                self.infer(interner, source, member, candidates);
            }
            for (id, cands) in structural_cands {
                candidates.entry(id).or_default().extend(cands);
            }
            self.visited.remove(&(source, target));
            return;
        }

        // M27 (conditional mode): a **template pattern** extends target
        // (`` `a-${infer R}` ``) matched against a string-literal check — non-greedy
        // anchored scanning captures each `infer` hole's segment as a NON-widened
        // string-literal candidate. Distribution over a union check is handled upstream
        // (the conditional distributes before the extends test), so the source here is a
        // single string literal.
        if self.union_target_descent && interner.store().tag(target) == TypeTag::Template {
            self.infer_from_template(interner, source, target, candidates);
            self.visited.remove(&(source, target));
            return;
        }

        if interner.store().tag(target) == TypeTag::Readonly {
            let Some(target_operand) = interner.store().readonly_operand(target) else {
                self.visited.remove(&(source, target));
                return;
            };
            let source_operand = if interner.store().tag(source) == TypeTag::Readonly {
                interner.store().readonly_operand(source).unwrap_or(source)
            } else {
                source
            };
            self.infer(interner, source_operand, target_operand, candidates);
            self.visited.remove(&(source, target));
            return;
        }

        if self.infer_identity_mapped_target(interner, source, target, candidates) {
            self.visited.remove(&(source, target));
            return;
        }

        match (interner.store().tag(source), interner.store().tag(target)) {
            (TypeTag::Object, TypeTag::Object) => {
                self.infer_objects(interner, source, target, candidates);
            }
            // B77: conditional `infer` over a callable object follows TypeScript's
            // last-overload rule. Call-site inference keeps its separate candidate
            // provenance policy.
            (TypeTag::Object, TypeTag::Function) if self.union_target_descent => {
                self.infer_object_call_signature(interner, source, target, candidates);
            }
            (TypeTag::Function, TypeTag::Function) => {
                self.infer_functions(interner, source, target, candidates);
            }
            (TypeTag::Union, TypeTag::Union) => {
                self.infer_unions(interner, source, target, candidates);
            }
            // M17: both arrays → recurse on the element (so a `T[]` parameter infers
            // `T` from a `number[]` argument, exactly as the object/function arms do
            // for their children). The element is itself interned, so this nests for
            // `T[][]`.
            (TypeTag::Array, TypeTag::Array) => {
                self.infer_arrays(interner, source, target, candidates);
            }
            // Conditional `infer`: tuple pairs recurse positionally, like function
            // parameters, through the shared inference machinery.
            (TypeTag::Tuple, TypeTag::Tuple) => {
                self.infer_tuples(interner, source, target, candidates);
            }
            // b57: a TUPLE source against an ARRAY target mirrors the relation's
            // tuple→array covariance — infer from each element into the array's element
            // position (same-name candidates union). Serves `[1, 2] extends (infer U)[]`
            // (type-level) and a tuple argument against a `T[]` parameter (call-site).
            (TypeTag::Tuple, TypeTag::Array) => {
                self.infer_tuple_into_array(interner, source, target, candidates);
            }
            // b57 (call-site only): an ARRAY source against a TUPLE target has NO
            // relation-rule mirror (an array is not assignable to a tuple), so it models
            // tsc contextually retyping a fresh array literal (`h([1, 2])` → `[T, T]`).
            // Excluded from conditional mode (`union_target_descent`) so an array never
            // matches a tuple `infer` pattern — `number[] extends [infer A, infer B]`
            // stays on the false branch.
            (TypeTag::Array, TypeTag::Tuple) if !self.union_target_descent => {
                self.infer_array_into_tuple(interner, source, target, candidates);
            }
            // Any other pairing (scalar, mismatched shapes, error type, …) yields
            // no candidate — inference simply learns nothing from it. Soundness is
            // preserved: the subsequent relation check still runs against whatever
            // the other parameters fixed to.
            _ => {}
        }

        self.visited.remove(&(source, target));
    }

    fn infer_identity_mapped_target(
        &mut self,
        interner: &mut Interner,
        source: TypeId,
        target: TypeId,
        candidates: &mut Candidates,
    ) -> bool {
        let Some(mapped) = interner.store().mapped_type(target).copied() else {
            return false;
        };
        if !mapped.homomorphic
            || mapped.modifiers_source.is_some()
            || mapped.optional_modifier != ModifierOp::Keep
            || mapped.readonly_modifier != ModifierOp::Keep
            || interner.store().tag(mapped.value_template) != TypeTag::MappedValue
            || interner.store().tag(mapped.key_source) != TypeTag::TypeParam
        {
            return false;
        }
        self.infer(interner, source, mapped.key_source, candidates);
        true
    }

    /// Both objects: recurse on the type of each property present in **both**. A
    /// property only on the source, or only on the target (e.g. a generic
    /// `{ value: T }` whose argument lacks `value`), contributes no candidate.
    fn infer_objects(
        &mut self,
        interner: &mut Interner,
        source: TypeId,
        target: TypeId,
        candidates: &mut Candidates,
    ) {
        // Snapshot the (name, type) pairs before recursing — the recursive `infer`
        // takes `&mut Interner` (it may intern a union while fixing widened
        // candidates), which cannot overlap the immutable side-table borrow.
        let Some(source_pairs) = property_pairs(interner.store(), source) else {
            return;
        };
        let Some(target_pairs) = property_pairs(interner.store(), target) else {
            return;
        };
        // `property_pairs` preserves the stable key order established by both object
        // interning paths. Reuse a same-key match so duplicate target keys retain
        // the prior first-source-member behavior.
        let mut source_cursor = 0;
        let mut previous_target_key = None;
        let mut previous_source_ty: Option<TypeId> = None;
        for (key, target_ty) in &target_pairs {
            #[cfg(test)]
            super::helpers::measure_inference(|measure| measure.object_target_properties += 1);
            let source_ty = if previous_target_key == Some(key) {
                previous_source_ty
            } else {
                let found = loop {
                    let Some((source_key, source_ty)) = source_pairs.get(source_cursor) else {
                        break None;
                    };
                    #[cfg(test)]
                    super::helpers::measure_inference(|measure| {
                        measure.object_source_property_comparisons += 1
                    });
                    match source_key.cmp(key) {
                        std::cmp::Ordering::Less => source_cursor += 1,
                        std::cmp::Ordering::Equal => {
                            source_cursor += 1;
                            break Some(*source_ty);
                        }
                        std::cmp::Ordering::Greater => break None,
                    }
                };
                previous_target_key = Some(key);
                previous_source_ty = found;
                found
            };
            if let Some(source_ty) = source_ty {
                self.infer(interner, source_ty, *target_ty, candidates);
            }
        }
    }

    /// Only the final call signature of a callable object contributes conditional
    /// inference candidates, matching `ReturnType`'s overload behavior.
    fn infer_object_call_signature(
        &mut self,
        interner: &mut Interner,
        source: TypeId,
        target: TypeId,
        candidates: &mut Candidates,
    ) {
        let signature = interner
            .store()
            .object_type(source)
            .and_then(|object| object.call_signatures.last())
            .copied();
        if let Some(signature) = signature {
            self.infer(interner, signature, target, candidates);
        }
    }

    /// Both arrays (M17): recurse on the **element** (`number[]` against `T[]`
    /// infers `T = number`). The element is interned, so the recursion is finite and
    /// nests naturally for `T[][]`.
    fn infer_arrays(
        &mut self,
        interner: &mut Interner,
        source: TypeId,
        target: TypeId,
        candidates: &mut Candidates,
    ) {
        let Some(source_elem) = interner.store().array_type(source).map(|a| a.element) else {
            return;
        };
        let Some(target_elem) = interner.store().array_type(target).map(|a| a.element) else {
            return;
        };
        self.infer(interner, source_elem, target_elem, candidates);
    }

    /// Both tuples (M25): recurse **positionally** on each element up to the shorter
    /// list. A `[infer H, infer R]` target infers `H`/`R` from a `[string, number]`
    /// source; a length mismatch just pairs the common prefix (sound — a surplus
    /// element contributes nothing).
    fn infer_tuples(
        &mut self,
        interner: &mut Interner,
        source: TypeId,
        target: TypeId,
        candidates: &mut Candidates,
    ) {
        let source_tuple = match interner.store().tuple_type(source) {
            Some(t) => t.clone(),
            None => return,
        };
        let target_tuple = match interner.store().tuple_type(target) {
            Some(t) => t.clone(),
            None => return,
        };
        let source_elems = source_tuple.elements;
        let Some(rest) = target_tuple.rest else {
            for (&source_ty, &target_ty) in source_elems.iter().zip(&target_tuple.elements) {
                self.infer(interner, source_ty, target_ty, candidates);
            }
            return;
        };
        if source_elems.len() < target_tuple.elements.len() {
            return;
        }
        for index in 0..rest.position {
            let Some(&source_ty) = source_elems.get(index) else {
                return;
            };
            let Some(&target_ty) = target_tuple.elements.get(index) else {
                return;
            };
            self.infer(interner, source_ty, target_ty, candidates);
        }
        let suffix_len = target_tuple.elements.len().saturating_sub(rest.position);
        let middle_end = source_elems.len().saturating_sub(suffix_len);
        for suffix_index in 0..suffix_len {
            let source_index = middle_end + suffix_index;
            let target_index = rest.position + suffix_index;
            let Some(&source_ty) = source_elems.get(source_index) else {
                return;
            };
            let Some(&target_ty) = target_tuple.elements.get(target_index) else {
                return;
            };
            self.infer(interner, source_ty, target_ty, candidates);
        }
        let middle = &source_elems[rest.position..middle_end];
        let rest_ty = interner
            .store()
            .readonly_operand(rest.ty)
            .unwrap_or(rest.ty);
        if interner.store().tag(rest_ty) == TypeTag::TypeParam {
            let captured = interner.intern_tuple(middle.to_vec());
            self.infer(interner, captured, rest_ty, candidates);
            return;
        }
        if let Some(array) = interner.store().array_type(rest_ty) {
            let elem = array.element;
            for &source_ty in middle {
                self.infer(interner, source_ty, elem, candidates);
            }
            return;
        }
        let captured = interner.intern_tuple(middle.to_vec());
        self.infer(interner, captured, rest_ty, candidates);
    }

    /// b57: infer tuple elements into an array target's element position. Empty
    /// tuples seed `never`, so `[] extends (infer U)[]` binds `U = never` instead
    /// of falling through to the unbound `unknown` fallback.
    fn infer_tuple_into_array(
        &mut self,
        interner: &mut Interner,
        source: TypeId,
        target: TypeId,
        candidates: &mut Candidates,
    ) {
        let source_elems: Vec<TypeId> = match interner.store().tuple_type(source) {
            Some(t) => t.elements.clone(),
            None => return,
        };
        let Some(target_elem) = interner.store().array_type(target).map(|a| a.element) else {
            return;
        };
        if source_elems.is_empty() {
            let never = interner.well_known().never;
            self.infer(interner, never, target_elem, candidates);
            return;
        }
        for source_ty in source_elems {
            self.infer(interner, source_ty, target_elem, candidates);
        }
    }

    /// b57 (call-site only) — an ARRAY source against a TUPLE target: infer the array's
    /// element type into every tuple element position. Models tsc contextually retyping
    /// a fresh array literal (`[1, 2]` inferred as `number[]`) against a tuple parameter
    /// (`[T, T]`); there is no relation rule to mirror (an array is not assignable to a
    /// tuple), so it is gated out of conditional mode by the caller.
    fn infer_array_into_tuple(
        &mut self,
        interner: &mut Interner,
        source: TypeId,
        target: TypeId,
        candidates: &mut Candidates,
    ) {
        let Some(source_elem) = interner.store().array_type(source).map(|a| a.element) else {
            return;
        };
        let target_elems: Vec<TypeId> = match interner.store().tuple_type(target) {
            Some(t) => t.elements.clone(),
            None => return,
        };
        for target_ty in target_elems {
            self.infer(interner, source_elem, target_ty, candidates);
        }
    }

    /// Match a string-literal `source` against a template-pattern `target` (M27),
    /// capturing each `infer` hole's segment (the hole is a **freshened type parameter**
    /// after the evaluator's de Bruijn freshening) as a NON-widened string-literal
    /// candidate. Non-capturing holes (`string`/`number` intrinsic, a literal) must match
    /// their segment; any mismatch (a failed anchor, an adjacent-hole separator, a bad
    /// numeric segment) records **no** candidate, so the conditional's extends test then
    /// takes the false branch. A non-string-literal source records nothing.
    fn infer_from_template(
        &mut self,
        interner: &mut Interner,
        source: TypeId,
        target: TypeId,
        candidates: &mut Candidates,
    ) {
        let s = match interner.store().literal_value(source) {
            Some(LiteralValue::String(s)) => s.clone(),
            _ => return,
        };
        let Some(template) = interner.store().template_type(target).cloned() else {
            return;
        };
        let TemplateType { texts, holes } = template;
        if holes.is_empty() {
            return;
        }
        let prefix = texts.first().map(String::as_str).unwrap_or("");
        let Some(mut rest) = s.strip_prefix(prefix) else {
            return;
        };
        let n = holes.len();
        // (param id, captured segment) pairs, interned once the whole scan succeeds.
        let mut captures: Vec<(TypeParamId, String)> = Vec::new();
        for (i, &hole) in holes.iter().enumerate() {
            let sep = texts.get(i + 1).map(String::as_str).unwrap_or("");
            let seg: &str = if i == n - 1 {
                let Some(seg) = rest.strip_suffix(sep) else {
                    return;
                };
                rest = "";
                seg
            } else {
                if sep.is_empty() {
                    return;
                }
                let Some(idx) = rest.find(sep) else {
                    return;
                };
                let seg = &rest[..idx];
                rest = &rest[idx + sep.len()..];
                seg
            };
            if interner.store().tag(hole) == TypeTag::TypeParam {
                if let Some(id) = interner.store().type_param(hole).map(|p| p.id) {
                    captures.push((id, seg.to_string()));
                }
            } else if !segment_matches_hole(interner.store(), hole, seg) {
                return;
            }
        }
        if !rest.is_empty() {
            return;
        }
        for (id, seg) in captures {
            let lit = interner.intern_literal(LiteralValue::String(seg));
            candidates.entry(id).or_default().push(lit);
        }
    }

    /// Both functions: recurse **positionally** on parameters (up to the shorter
    /// list) and on the return type. Parameter pairing is positional because that
    /// is how the relation engine compares function parameters (M3); a surplus
    /// parameter on either side contributes nothing.
    fn infer_functions(
        &mut self,
        interner: &mut Interner,
        source: TypeId,
        target: TypeId,
        candidates: &mut Candidates,
    ) {
        let Some(source_fn) = interner.store().function_type(source).cloned() else {
            return;
        };
        let Some(target_fn) = interner.store().function_type(target).cloned() else {
            return;
        };
        if let (Some(source_receiver), Some(target_receiver)) =
            (source_fn.receiver, target_fn.receiver)
        {
            self.infer(interner, source_receiver, target_receiver, candidates);
        }
        if let Some(rest_ty) = direct_infer_rest(&target_fn) {
            let captured = source_parameter_tuple(interner, &source_fn);
            self.infer(interner, captured, rest_ty, candidates);
        } else if let (Some((source_params, _)), Some((target_params, _))) = (
            function_shape(interner.store(), source),
            function_shape(interner.store(), target),
        ) {
            for (&source_ty, &target_ty) in source_params.iter().zip(&target_params) {
                self.infer(interner, source_ty, target_ty, candidates);
            }
        }
        self.infer(interner, source_fn.ret, target_fn.ret, candidates);
    }

    /// Both unions: a best-effort pairwise match. Only when the two unions have the
    /// **same number of members** are the (canonical-order) members paired
    /// positionally and recursed; an unequal count is ambiguous, so nothing is
    /// inferred (sound — skipping never invents a wrong candidate). This is kept
    /// intentionally simple for M10; precise union inference is a later milestone.
    fn infer_unions(
        &mut self,
        interner: &mut Interner,
        source: TypeId,
        target: TypeId,
        candidates: &mut Candidates,
    ) {
        let source_members: Vec<TypeId> = match interner.store().union_members(source) {
            Some(m) => m.to_vec(),
            None => return,
        };
        let target_members: Vec<TypeId> = match interner.store().union_members(target) {
            Some(m) => m.to_vec(),
            None => return,
        };
        if source_members.len() != target_members.len() {
            return;
        }
        for (&source_ty, &target_ty) in source_members.iter().zip(&target_members) {
            self.infer(interner, source_ty, target_ty, candidates);
        }
    }
}

fn direct_infer_rest(function: &FunctionType) -> Option<TypeId> {
    if function.total_fixed_param_count() != 0 {
        return None;
    }
    if function.params.iter().filter(|param| param.rest).count() != 1 {
        return None;
    }
    let rest = function.rest_param()?;
    Some(rest.ty)
}

fn source_parameter_tuple(interner: &mut Interner, function: &FunctionType) -> TypeId {
    let fixed: Vec<TypeId> = function.fixed_params().map(|param| param.ty).collect();
    if let Some(rest) = function.rest_param() {
        interner.intern_tuple_type(TupleType::with_rest(
            fixed,
            TupleRestType::new(function.total_fixed_param_count(), rest.ty),
        ))
    } else {
        interner.intern_tuple(fixed)
    }
}

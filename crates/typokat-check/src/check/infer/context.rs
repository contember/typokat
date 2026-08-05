use super::helpers::{function_shape, property_pairs, segment_matches_hole};
use super::*;
use crate::types::repr::{
    DeclaredRecipeNode, FunctionType, LiteralValue, ModifierOp, TemplateType, TupleRestType,
    TupleType,
};
use crate::types::{DerivationEdge, DerivedType};

/// The per-inference recursion state: the cycle guard for structural matching, plus
/// the **collection mode** (M25). Built once per entry-point call and dropped after.
pub(super) struct InferenceContext<'query> {
    /// `(source, target)` id pairs currently on the recursion stack. Re-entering a
    /// pair short-circuits (it is already contributing candidates further up). See
    /// the module docs.
    visited: FxHashSet<(TypeId, TypeId)>,
    source_stack: Vec<RecursionFrame>,
    target_stack: Vec<RecursionFrame>,
    source_expanding: bool,
    target_expanding: bool,
    mode: InferenceMode,
    normalization: Option<&'query dyn crate::relate::RelationNormalization>,
    active_params: Option<&'query FxHashSet<TypeParamId>>,
    exhaustion: Option<crate::class_semantics::Exhaustion>,
    demands: Vec<crate::relate::RelationDemand>,
    demand_set: FxHashSet<crate::relate::RelationDemand>,
}

struct RecursionFrame {
    ty: TypeId,
    identity: TypeId,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InferenceMode {
    Conditional,
    CallSite,
}

fn recursion_identity(interner: &Interner, raw: TypeId, normalized: DerivedType) -> TypeId {
    let declared_template = interner
        .store()
        .declared_type(raw)
        .and_then(|declared| interner.store().declared_recipe(declared.recipe))
        .and_then(|recipe| match &recipe.node {
            DeclaredRecipeNode::Application { template, .. } => Some(*template),
            _ => None,
        });
    declared_template.unwrap_or_else(|| interner.derivation_identity(normalized))
}

fn is_deeply_repeating(stack: &[RecursionFrame]) -> bool {
    const MAX_DEPTH: usize = 2;
    if stack.len() < MAX_DEPTH {
        return false;
    }
    let Some(current) = stack.last() else {
        return false;
    };
    let mut count = 0;
    let mut last_type = TypeId(0);
    for frame in stack {
        if frame.identity == current.identity {
            if frame.ty >= last_type {
                count += 1;
                if count >= MAX_DEPTH {
                    return true;
                }
            }
            last_type = frame.ty;
        }
    }
    false
}

fn unwrap_declared_type_recipe(interner: &Interner, mut derived: DerivedType) -> DerivedType {
    let mut seen = FxHashSet::default();
    while seen.insert(derived.ty) {
        let Some(application) = interner.store().declared_type(derived.ty) else {
            break;
        };
        let Some(recipe) = interner.store().declared_recipe(application.recipe) else {
            break;
        };
        let DeclaredRecipeNode::Type(ty) = &recipe.node else {
            break;
        };
        let ty = *ty;
        let mapped = interner.store().type_param(ty).and_then(|parameter| {
            let index = application
                .mapper
                .binary_search_by_key(&parameter.id, |(parameter, _)| *parameter)
                .ok()?;
            application
                .mapper
                .get(index)
                .map(|(_, mapped)| (index, *mapped))
        });
        let next_ty = mapped.map_or(ty, |(_, mapped)| mapped);
        if next_ty == derived.ty {
            break;
        }
        derived = mapped.map_or_else(
            || DerivedType::plain(ty),
            |(index, mapped)| {
                interner.derivation_child(derived, DerivationEdge::DeclaredMapper(index), mapped)
            },
        );
    }
    derived
}

impl<'query> InferenceContext<'query> {
    /// Conditional-`infer` mode (M25): a union extends target collects from its
    /// members; a template-pattern target captures `infer` holes (M27).
    pub(super) fn for_conditional() -> Self {
        InferenceContext {
            visited: FxHashSet::default(),
            source_stack: Vec::new(),
            target_stack: Vec::new(),
            source_expanding: false,
            target_expanding: false,
            mode: InferenceMode::Conditional,
            normalization: None,
            active_params: None,
            exhaustion: None,
            demands: Vec::new(),
            demand_set: FxHashSet::default(),
        }
    }

    pub(super) fn for_query(
        normalization: &'query dyn crate::relate::RelationNormalization,
        active_params: Option<&'query FxHashSet<TypeParamId>>,
    ) -> Self {
        InferenceContext {
            visited: FxHashSet::default(),
            source_stack: Vec::new(),
            target_stack: Vec::new(),
            source_expanding: false,
            target_expanding: false,
            mode: InferenceMode::CallSite,
            normalization: Some(normalization),
            active_params,
            exhaustion: None,
            demands: Vec::new(),
            demand_set: FxHashSet::default(),
        }
    }

    pub(super) fn take_exhaustion(&mut self) -> Option<crate::class_semantics::Exhaustion> {
        self.exhaustion.take()
    }

    pub(super) fn take_demands(&mut self) -> Vec<crate::relate::RelationDemand> {
        std::mem::take(&mut self.demands)
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
        self.infer_derived(
            interner,
            DerivedType::plain(source),
            DerivedType::plain(target),
            candidates,
        );
    }

    pub(super) fn infer_derived(
        &mut self,
        interner: &mut Interner,
        source: DerivedType,
        target: DerivedType,
        candidates: &mut Candidates,
    ) {
        if self.exhaustion.is_some() {
            return;
        }
        let source = self.unwrap_transparent_declared_recipe(interner, source);
        let target = self.unwrap_transparent_declared_recipe(interner, target);
        let raw_source = source.ty;
        let raw_target = target.ty;
        // Declared applications retain their generic recipe only on the raw nodes.
        // Guard this match before normalization can replace either node with its
        // structural materialization; mapper recursion still normalizes its children.
        if !self.visited.insert((source.ty, target.ty)) {
            return;
        }
        let same_declared_recipe =
            self.infer_same_declared_recipe(interner, source, target, candidates);
        let same_declared_application_head = !same_declared_recipe
            && self.infer_same_declared_application_head(interner, source, target, candidates);
        self.visited.remove(&(source.ty, target.ty));
        if same_declared_recipe {
            return;
        }
        if same_declared_application_head {
            return;
        }
        let (source, target) = if let Some(normalization) = self.normalization {
            let source = match normalization.normalize_derived(interner.store(), source) {
                Ok(source) => source,
                Err(reason) => {
                    self.exhaustion = Some(reason);
                    return;
                }
            };
            let target = match normalization.normalize_derived(interner.store(), target) {
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
        let target_param = interner.store().type_param(target.ty).map(|param| param.id);
        if target_param.is_some_and(|id| {
            self.active_params
                .is_some_and(|active| !active.contains(&id))
        }) {
            return;
        }
        if let Some(normalization) = self.normalization {
            let mut unresolved = false;
            for derived in [source, target] {
                if let Some(demand) =
                    normalization.relation_demand_derived(interner.store(), derived)
                {
                    unresolved = true;
                    if self.demand_set.insert(demand) {
                        self.demands.push(demand);
                    }
                }
            }
            if unresolved {
                return;
            }
        }

        // A target type parameter is the one place a candidate is recorded — always
        // AS-IS (raw). Call-site widening happens at fix time (per
        // parameter); conditional-`infer` extraction never widens (tsc keeps `"x"` —
        // M25 review HIGH-1/HIGH-2).
        if let Some(id) = target_param {
            // Recovery is not evidence: keep a declaration default available.
            if self.mode == InferenceMode::CallSite && source.ty == interner.well_known().error {
                return;
            }
            candidates.entry(id).or_default().push(source.ty);
            return;
        }

        // Structural recursion is cycle-guarded: re-entering an in-flight
        // (source, target) pair adds nothing, so short-circuit.
        if !self.visited.insert((source.ty, target.ty)) {
            return;
        }

        // Call-site inference treats `null`/`undefined` as non-inference members when a
        // nullable union has exactly one substantive target. Contextual argument walking
        // uses the same target, so fresh literals retain their structural candidates.
        if self.mode == InferenceMode::CallSite && interner.store().tag(target.ty) == TypeTag::Union
        {
            let substantive = interner
                .store()
                .union_members(target.ty)
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
                self.infer_derived(interner, source, DerivedType::plain(*member), candidates);
                self.visited.remove(&(source.ty, target.ty));
                return;
            }
        }

        // Structural union members contribute first. A naked target parameter is
        // lower priority and contributes only when no structural member bound it.
        if interner.store().tag(target.ty) == TypeTag::Union
            && interner.store().tag(source.ty) != TypeTag::Union
        {
            let members: Vec<TypeId> = interner
                .store()
                .union_members(target.ty)
                .map(|m| m.to_vec())
                .unwrap_or_default();
            let (naked, structural): (Vec<TypeId>, Vec<TypeId>) = members
                .into_iter()
                .partition(|&m| interner.store().tag(m) == TypeTag::TypeParam);
            let mut structural_cands = Candidates::default();
            for member in structural {
                self.infer_derived(
                    interner,
                    source,
                    DerivedType::plain(member),
                    &mut structural_cands,
                );
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
                self.infer_derived(interner, source, DerivedType::plain(member), candidates);
            }
            for (id, cands) in structural_cands {
                candidates.entry(id).or_default().extend(cands);
            }
            self.visited.remove(&(source.ty, target.ty));
            return;
        }

        // M27 (conditional mode): a **template pattern** extends target
        // (`` `a-${infer R}` ``) matched against a string-literal check — non-greedy
        // anchored scanning captures each `infer` hole's segment as a NON-widened
        // string-literal candidate. Distribution over a union check is handled upstream
        // (the conditional distributes before the extends test), so the source here is a
        // single string literal.
        if self.mode == InferenceMode::Conditional
            && interner.store().tag(target.ty) == TypeTag::Template
        {
            self.infer_from_template(interner, source.ty, target.ty, candidates);
            self.visited.remove(&(source.ty, target.ty));
            return;
        }

        if interner.store().tag(target.ty) == TypeTag::Readonly {
            let Some(target_operand) = interner.store().readonly_operand(target.ty) else {
                self.visited.remove(&(source.ty, target.ty));
                return;
            };
            let source_operand = if interner.store().tag(source.ty) == TypeTag::Readonly {
                interner
                    .store()
                    .readonly_operand(source.ty)
                    .unwrap_or(source.ty)
            } else {
                source.ty
            };
            self.infer_derived(
                interner,
                interner.derivation_child(source, DerivationEdge::ReadonlyOperand, source_operand),
                interner.derivation_child(target, DerivationEdge::ReadonlyOperand, target_operand),
                candidates,
            );
            self.visited.remove(&(source.ty, target.ty));
            return;
        }

        if self.infer_identity_mapped_target(interner, source, target, candidates) {
            self.visited.remove(&(source.ty, target.ty));
            return;
        }

        match (
            interner.store().tag(source.ty),
            interner.store().tag(target.ty),
        ) {
            (TypeTag::Object, TypeTag::Object) => {
                self.infer_structural_pair(
                    interner, raw_source, source, raw_target, target, candidates,
                );
            }
            // B77: conditional `infer` over a callable object follows TypeScript's
            // last-overload rule. Call-site inference keeps its separate candidate
            // provenance policy.
            (TypeTag::Object, TypeTag::Function) if self.mode == InferenceMode::Conditional => {
                self.infer_structural_pair(
                    interner, raw_source, source, raw_target, target, candidates,
                );
            }
            (TypeTag::Function, TypeTag::Function) => {
                self.infer_structural_pair(
                    interner, raw_source, source, raw_target, target, candidates,
                );
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
            (TypeTag::Array, TypeTag::Tuple) if self.mode == InferenceMode::CallSite => {
                self.infer_array_into_tuple(interner, source, target, candidates);
            }
            // Any other pairing (scalar, mismatched shapes, error type, …) yields
            // no candidate — inference simply learns nothing from it. Soundness is
            // preserved: the subsequent relation check still runs against whatever
            // the other parameters fixed to.
            _ => {}
        }

        self.visited.remove(&(source.ty, target.ty));
    }

    fn unwrap_transparent_declared_recipe(
        &self,
        interner: &Interner,
        derived: DerivedType,
    ) -> DerivedType {
        let Some(normalization) = self.normalization else {
            return unwrap_declared_type_recipe(interner, derived);
        };
        match normalization.normalize_derived(interner.store(), derived) {
            Ok(normalized) if normalized == derived => {
                unwrap_declared_type_recipe(interner, derived)
            }
            _ => derived,
        }
    }

    fn infer_structural_pair(
        &mut self,
        interner: &mut Interner,
        raw_source: TypeId,
        source: DerivedType,
        raw_target: TypeId,
        target: DerivedType,
        candidates: &mut Candidates,
    ) {
        let source_identity = recursion_identity(interner, raw_source, source);
        let target_identity = recursion_identity(interner, raw_target, target);
        self.source_stack.push(RecursionFrame {
            ty: source.ty,
            identity: source_identity,
        });
        self.target_stack.push(RecursionFrame {
            ty: target.ty,
            identity: target_identity,
        });

        let saved_source_expanding = self.source_expanding;
        let saved_target_expanding = self.target_expanding;
        self.source_expanding |= is_deeply_repeating(&self.source_stack);
        self.target_expanding |= is_deeply_repeating(&self.target_stack);
        if !(self.source_expanding && self.target_expanding) {
            match (
                interner.store().tag(source.ty),
                interner.store().tag(target.ty),
            ) {
                (TypeTag::Object, TypeTag::Object) => {
                    self.infer_objects(interner, source, target, candidates);
                }
                (TypeTag::Object, TypeTag::Function) if self.mode == InferenceMode::Conditional => {
                    self.infer_object_call_signature(interner, source, target, candidates);
                }
                (TypeTag::Function, TypeTag::Function) => {
                    self.infer_functions(interner, source, target, candidates);
                }
                _ => {}
            }
        }
        self.source_expanding = saved_source_expanding;
        self.target_expanding = saved_target_expanding;
        self.target_stack.pop();
        self.source_stack.pop();
    }

    fn infer_same_declared_recipe(
        &mut self,
        interner: &mut Interner,
        source: DerivedType,
        target: DerivedType,
        candidates: &mut Candidates,
    ) -> bool {
        let Some(source_declared) = interner.store().declared_type(source.ty).cloned() else {
            return false;
        };
        let Some(target_declared) = interner.store().declared_type(target.ty).cloned() else {
            return false;
        };
        if source_declared.recipe != target_declared.recipe {
            return false;
        }
        let Some(recipe) = interner
            .store()
            .declared_recipe(source_declared.recipe)
            .cloned()
        else {
            return false;
        };
        if source_declared.mapper.len() != recipe.free_params.len()
            || target_declared.mapper.len() != recipe.free_params.len()
            || source_declared
                .mapper
                .iter()
                .map(|(parameter, _)| *parameter)
                .ne(recipe.free_params.iter().copied())
            || target_declared
                .mapper
                .iter()
                .map(|(parameter, _)| *parameter)
                .ne(recipe.free_params.iter().copied())
        {
            return false;
        }
        for (index, ((_, source_ty), (_, target_ty))) in source_declared
            .mapper
            .into_iter()
            .zip(target_declared.mapper)
            .enumerate()
        {
            let source_child =
                interner.derivation_child(source, DerivationEdge::DeclaredMapper(index), source_ty);
            let target_child =
                interner.derivation_child(target, DerivationEdge::DeclaredMapper(index), target_ty);
            self.infer_derived(interner, source_child, target_child, candidates);
        }
        true
    }

    fn infer_same_declared_application_head(
        &mut self,
        interner: &mut Interner,
        source: DerivedType,
        target: DerivedType,
        candidates: &mut Candidates,
    ) -> bool {
        let Some(source_declared) = interner.store().declared_type(source.ty).cloned() else {
            return false;
        };
        let Some(target_declared) = interner.store().declared_type(target.ty).cloned() else {
            return false;
        };
        let Some(source_recipe) = interner
            .store()
            .declared_recipe(source_declared.recipe)
            .cloned()
        else {
            return false;
        };
        let Some(target_recipe) = interner
            .store()
            .declared_recipe(target_declared.recipe)
            .cloned()
        else {
            return false;
        };
        let (
            crate::types::repr::DeclaredRecipeNode::Application {
                template: source_template,
                parameters: source_parameters,
                arguments: source_arguments,
            },
            crate::types::repr::DeclaredRecipeNode::Application {
                template: target_template,
                parameters: target_parameters,
                arguments: target_arguments,
            },
        ) = (source_recipe.node, target_recipe.node)
        else {
            return false;
        };
        if source_template != target_template
            || source_parameters != target_parameters
            || source_arguments.len() != target_arguments.len()
        {
            return false;
        }
        for (index, (source_argument, target_argument)) in source_arguments
            .into_iter()
            .zip(target_arguments)
            .enumerate()
        {
            let source_child_ty =
                interner.intern_declared(source_argument, source_declared.mapper.clone());
            let target_child_ty =
                interner.intern_declared(target_argument, target_declared.mapper.clone());
            let source_child = interner.derivation_child(
                source,
                DerivationEdge::DeclaredArgument(index),
                source_child_ty,
            );
            let target_child = interner.derivation_child(
                target,
                DerivationEdge::DeclaredArgument(index),
                target_child_ty,
            );
            self.infer_derived(interner, source_child, target_child, candidates);
        }
        true
    }

    fn infer_identity_mapped_target(
        &mut self,
        interner: &mut Interner,
        source: DerivedType,
        target: DerivedType,
        candidates: &mut Candidates,
    ) -> bool {
        let Some(mapped) = interner.store().mapped_type(target.ty).copied() else {
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
        let target_key =
            interner.derivation_child(target, DerivationEdge::MappedKeySource, mapped.key_source);
        self.infer_derived(interner, source, target_key, candidates);
        true
    }

    /// Both objects: recurse on the type of each property present in **both**. A
    /// property only on the source, or only on the target (e.g. a generic
    /// `{ value: T }` whose argument lacks `value`), contributes no candidate.
    fn infer_objects(
        &mut self,
        interner: &mut Interner,
        source: DerivedType,
        target: DerivedType,
        candidates: &mut Candidates,
    ) {
        // Snapshot the (name, type) pairs before recursing — the recursive `infer`
        // takes `&mut Interner` (it may intern a union while fixing widened
        // candidates), which cannot overlap the immutable side-table borrow.
        let Some(source_pairs) = property_pairs(interner.store(), source.ty) else {
            return;
        };
        let Some(target_pairs) = property_pairs(interner.store(), target.ty) else {
            return;
        };
        // `property_pairs` preserves the stable key order established by both object
        // interning paths. Reuse a same-key match so duplicate target keys retain
        // the prior first-source-member behavior.
        let mut source_cursor = 0;
        let mut previous_target_key = None;
        let mut previous_source_ty: Option<(usize, TypeId)> = None;
        for (target_index, (key, target_ty)) in target_pairs.iter().enumerate() {
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
                            break Some((source_cursor - 1, *source_ty));
                        }
                        std::cmp::Ordering::Greater => break None,
                    }
                };
                previous_target_key = Some(key);
                previous_source_ty = found;
                found
            };
            if let Some((source_index, source_ty)) = source_ty {
                let source_child = interner.derivation_child(
                    source,
                    DerivationEdge::ObjectProperty(source_index),
                    source_ty,
                );
                let target_child = interner.derivation_child(
                    target,
                    DerivationEdge::ObjectProperty(target_index),
                    *target_ty,
                );
                self.infer_derived(interner, source_child, target_child, candidates);
            }
        }
    }

    /// Only the final call signature of a callable object contributes conditional
    /// inference candidates, matching `ReturnType`'s overload behavior.
    fn infer_object_call_signature(
        &mut self,
        interner: &mut Interner,
        source: DerivedType,
        target: DerivedType,
        candidates: &mut Candidates,
    ) {
        let signature = interner
            .store()
            .object_type(source.ty)
            .and_then(|object| object.call_signatures.last())
            .copied();
        if let Some(signature) = signature {
            let index = interner
                .store()
                .object_type(source.ty)
                .map_or(0, |object| object.call_signatures.len().saturating_sub(1));
            let source_child = interner.derivation_child(
                source,
                DerivationEdge::ObjectCallSignature(index),
                signature,
            );
            self.infer_derived(interner, source_child, target, candidates);
        }
    }

    /// Both arrays (M17): recurse on the **element** (`number[]` against `T[]`
    /// infers `T = number`). The element is interned, so the recursion is finite and
    /// nests naturally for `T[][]`.
    fn infer_arrays(
        &mut self,
        interner: &mut Interner,
        source: DerivedType,
        target: DerivedType,
        candidates: &mut Candidates,
    ) {
        let Some(source_elem) = interner.store().array_type(source.ty).map(|a| a.element) else {
            return;
        };
        let Some(target_elem) = interner.store().array_type(target.ty).map(|a| a.element) else {
            return;
        };
        let source_child =
            interner.derivation_child(source, DerivationEdge::ArrayElement, source_elem);
        let target_child =
            interner.derivation_child(target, DerivationEdge::ArrayElement, target_elem);
        self.infer_derived(interner, source_child, target_child, candidates);
    }

    /// Both tuples (M25): recurse **positionally** on each element up to the shorter
    /// list. A `[infer H, infer R]` target infers `H`/`R` from a `[string, number]`
    /// source; a length mismatch just pairs the common prefix (sound — a surplus
    /// element contributes nothing).
    fn infer_tuples(
        &mut self,
        interner: &mut Interner,
        source: DerivedType,
        target: DerivedType,
        candidates: &mut Candidates,
    ) {
        let source_tuple = match interner.store().tuple_type(source.ty) {
            Some(t) => t.clone(),
            None => return,
        };
        let target_tuple = match interner.store().tuple_type(target.ty) {
            Some(t) => t.clone(),
            None => return,
        };
        let source_elems = source_tuple.elements;
        let Some(rest) = target_tuple.rest else {
            for (index, (&source_ty, &target_ty)) in
                source_elems.iter().zip(&target_tuple.elements).enumerate()
            {
                let source_child = interner.derivation_child(
                    source,
                    DerivationEdge::TupleElement(index),
                    source_ty,
                );
                let target_child = interner.derivation_child(
                    target,
                    DerivationEdge::TupleElement(index),
                    target_ty,
                );
                self.infer_derived(interner, source_child, target_child, candidates);
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
            let source_child =
                interner.derivation_child(source, DerivationEdge::TupleElement(index), source_ty);
            let target_child =
                interner.derivation_child(target, DerivationEdge::TupleElement(index), target_ty);
            self.infer_derived(interner, source_child, target_child, candidates);
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
            let source_child = interner.derivation_child(
                source,
                DerivationEdge::TupleElement(source_index),
                source_ty,
            );
            let target_child = interner.derivation_child(
                target,
                DerivationEdge::TupleElement(target_index),
                target_ty,
            );
            self.infer_derived(interner, source_child, target_child, candidates);
        }
        let middle = source_elems[rest.position..middle_end]
            .iter()
            .copied()
            .enumerate()
            .map(|(offset, ty)| {
                interner.derivation_child(
                    source,
                    DerivationEdge::TupleElement(rest.position + offset),
                    ty,
                )
            })
            .collect::<Vec<_>>();
        let mut rest_derived =
            interner.derivation_child(target, DerivationEdge::TupleRest, rest.ty);
        let rest_ty = interner
            .store()
            .readonly_operand(rest.ty)
            .unwrap_or(rest.ty);
        if rest_ty != rest.ty {
            rest_derived =
                interner.derivation_child(rest_derived, DerivationEdge::ReadonlyOperand, rest_ty);
        }
        if interner.store().tag(rest_ty) == TypeTag::TypeParam {
            let captured = interner.intern_derived_tuple(&middle);
            self.infer_derived(interner, captured, rest_derived, candidates);
            return;
        }
        if let Some(array) = interner.store().array_type(rest_ty) {
            let target_element = interner.derivation_child(
                rest_derived,
                DerivationEdge::ArrayElement,
                array.element,
            );
            for source_child in middle {
                self.infer_derived(interner, source_child, target_element, candidates);
            }
            return;
        }
        let captured = interner.intern_derived_tuple(&middle);
        self.infer_derived(interner, captured, rest_derived, candidates);
    }

    /// b57: infer tuple elements into an array target's element position. Empty
    /// tuples seed `never`, so `[] extends (infer U)[]` binds `U = never` instead
    /// of falling through to the unbound `unknown` fallback.
    fn infer_tuple_into_array(
        &mut self,
        interner: &mut Interner,
        source: DerivedType,
        target: DerivedType,
        candidates: &mut Candidates,
    ) {
        let source_elems: Vec<TypeId> = match interner.store().tuple_type(source.ty) {
            Some(t) => t.elements.clone(),
            None => return,
        };
        let Some(target_elem) = interner.store().array_type(target.ty).map(|a| a.element) else {
            return;
        };
        let target_child =
            interner.derivation_child(target, DerivationEdge::ArrayElement, target_elem);
        if source_elems.is_empty() {
            let never = interner.well_known().never;
            self.infer_derived(
                interner,
                DerivedType::plain(never),
                target_child,
                candidates,
            );
            return;
        }
        for (index, source_ty) in source_elems.into_iter().enumerate() {
            let source_child =
                interner.derivation_child(source, DerivationEdge::TupleElement(index), source_ty);
            self.infer_derived(interner, source_child, target_child, candidates);
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
        source: DerivedType,
        target: DerivedType,
        candidates: &mut Candidates,
    ) {
        let Some(source_elem) = interner.store().array_type(source.ty).map(|a| a.element) else {
            return;
        };
        let source_child =
            interner.derivation_child(source, DerivationEdge::ArrayElement, source_elem);
        let target_elems: Vec<TypeId> = match interner.store().tuple_type(target.ty) {
            Some(t) => t.elements.clone(),
            None => return,
        };
        for (index, target_ty) in target_elems.into_iter().enumerate() {
            let target_child =
                interner.derivation_child(target, DerivationEdge::TupleElement(index), target_ty);
            self.infer_derived(interner, source_child, target_child, candidates);
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
        source: DerivedType,
        target: DerivedType,
        candidates: &mut Candidates,
    ) {
        let Some(source_fn) = interner.store().function_type(source.ty).cloned() else {
            return;
        };
        let Some(target_fn) = interner.store().function_type(target.ty).cloned() else {
            return;
        };
        if let (Some(source_receiver), Some(target_receiver)) =
            (source_fn.receiver, target_fn.receiver)
        {
            let source_child = interner.derivation_child(
                source,
                DerivationEdge::FunctionReceiver,
                source_receiver,
            );
            let target_child = interner.derivation_child(
                target,
                DerivationEdge::FunctionReceiver,
                target_receiver,
            );
            self.infer_derived(interner, source_child, target_child, candidates);
        }
        if let Some(rest_ty) = direct_infer_rest(&target_fn) {
            let captured = source_parameter_tuple(interner, &source_fn);
            self.infer(interner, captured, rest_ty, candidates);
        } else if let (Some((source_params, _)), Some((target_params, _))) = (
            function_shape(interner.store(), source.ty),
            function_shape(interner.store(), target.ty),
        ) {
            for (index, (&source_ty, &target_ty)) in
                source_params.iter().zip(&target_params).enumerate()
            {
                let source_child = interner.derivation_child(
                    source,
                    DerivationEdge::FunctionParameter(index),
                    source_ty,
                );
                let target_child = interner.derivation_child(
                    target,
                    DerivationEdge::FunctionParameter(index),
                    target_ty,
                );
                self.infer_derived(interner, source_child, target_child, candidates);
            }
        }
        let source_ret =
            interner.derivation_child(source, DerivationEdge::FunctionReturn, source_fn.ret);
        let target_ret =
            interner.derivation_child(target, DerivationEdge::FunctionReturn, target_fn.ret);
        self.infer_derived(interner, source_ret, target_ret, candidates);
    }

    /// Both unions: a best-effort pairwise match. Only when the two unions have the
    /// **same number of members** are the (canonical-order) members paired
    /// positionally and recursed; an unequal count is ambiguous, so nothing is
    /// inferred (sound — skipping never invents a wrong candidate). This is kept
    /// intentionally simple for M10; precise union inference is a later milestone.
    fn infer_unions(
        &mut self,
        interner: &mut Interner,
        source: DerivedType,
        target: DerivedType,
        candidates: &mut Candidates,
    ) {
        let source_members: Vec<TypeId> = match interner.store().union_members(source.ty) {
            Some(m) => m.to_vec(),
            None => return,
        };
        let target_members: Vec<TypeId> = match interner.store().union_members(target.ty) {
            Some(m) => m.to_vec(),
            None => return,
        };
        if source_members.len() != target_members.len() {
            return;
        }
        for (index, (&source_ty, &target_ty)) in
            source_members.iter().zip(&target_members).enumerate()
        {
            let source_child =
                interner.derivation_child(source, DerivationEdge::UnionMember(index), source_ty);
            let target_child =
                interner.derivation_child(target, DerivationEdge::UnionMember(index), target_ty);
            self.infer_derived(interner, source_child, target_child, candidates);
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

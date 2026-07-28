use super::*;

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct InferRewriteMeasure {
    pub top_level_runs: u64,
    pub visits: u64,
    pub memo_hits: u64,
    pub memo_inserts: u64,
    pub reentries: u64,
    pub tainted_identity_returns: u64,
}

#[cfg(test)]
thread_local! {
    static INFER_REWRITE_MEASURE: std::cell::RefCell<InferRewriteMeasure> = std::cell::RefCell::new(InferRewriteMeasure::default());
}

#[cfg(test)]
pub(super) fn reset_infer_rewrite_measure() {
    INFER_REWRITE_MEASURE.with(|measure| *measure.borrow_mut() = InferRewriteMeasure::default());
}

#[cfg(test)]
pub(super) fn infer_rewrite_measure() -> InferRewriteMeasure {
    INFER_REWRITE_MEASURE.with(|measure| *measure.borrow())
}

#[cfg(test)]
fn measure_infer_rewrite(update: impl FnOnce(&mut InferRewriteMeasure)) {
    INFER_REWRITE_MEASURE.with(|measure| update(&mut measure.borrow_mut()));
}

/// Per-top-level context for conditional `infer` binder freshening. The fresh binder
/// slice is lexical to one conditional, so neither the cycle guard nor completed
/// results may outlive this rewrite invocation.
struct InferRewrite<'a> {
    fresh: &'a [TypeId],
    in_progress: FxHashSet<TypeId>,
    active: Vec<TypeId>,
    cycle_tainted: FxHashSet<TypeId>,
    memo: FxHashMap<TypeId, TypeId>,
}

/// One pending post-order rewrite. Every frame owns its store snapshot so the
/// interner can be mutably borrowed while the frame is rebuilt.
enum InferRewriteFrame {
    Identity {
        ty: TypeId,
        result: TypeId,
    },
    Object {
        ty: TypeId,
        object: ObjectType,
    },
    Function {
        ty: TypeId,
        function: FunctionType,
    },
    Union {
        ty: TypeId,
        members: Vec<TypeId>,
    },
    Intersection {
        ty: TypeId,
        members: Vec<TypeId>,
    },
    Array {
        ty: TypeId,
        element: TypeId,
    },
    Tuple {
        ty: TypeId,
        tuple: TupleType,
    },
    Readonly {
        ty: TypeId,
        operand: TypeId,
    },
    Instantiation {
        ty: TypeId,
        base: TypeId,
        args: Vec<(TypeParamId, TypeId)>,
    },
    ClassInstance {
        ty: TypeId,
        class: crate::types::ClassId,
        args: Vec<TypeId>,
    },
    Template {
        ty: TypeId,
        template: TemplateType,
    },
    Keyof {
        ty: TypeId,
        operand: TypeId,
    },
    DeferredIndexedAccess {
        ty: TypeId,
        object: TypeId,
        index: TypeId,
    },
}

impl InferRewriteFrame {
    fn ty(&self) -> TypeId {
        match self {
            InferRewriteFrame::Identity { ty, .. }
            | InferRewriteFrame::Object { ty, .. }
            | InferRewriteFrame::Function { ty, .. }
            | InferRewriteFrame::Union { ty, .. }
            | InferRewriteFrame::Intersection { ty, .. }
            | InferRewriteFrame::Array { ty, .. }
            | InferRewriteFrame::Tuple { ty, .. }
            | InferRewriteFrame::Readonly { ty, .. }
            | InferRewriteFrame::Instantiation { ty, .. }
            | InferRewriteFrame::ClassInstance { ty, .. }
            | InferRewriteFrame::Template { ty, .. }
            | InferRewriteFrame::Keyof { ty, .. }
            | InferRewriteFrame::DeferredIndexedAccess { ty, .. } => *ty,
        }
    }
}

enum InferRewriteTask {
    Visit(TypeId),
    Finish {
        frame: InferRewriteFrame,
        child_count: usize,
    },
}

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
    ) {
        let Some(inst) = self.interner.store().instantiation_type(ty).cloned() else {
            values.push(ty);
            return;
        };
        if self.in_flight.contains(&ty) {
            self.note_cycle();
            return;
        }
        if self.exhausted {
            return;
        }

        // M28/WU3 — a **string-intrinsic** instantiation (`Uppercase<…>`), intercepted
        // by the prelude-declared marker identity: evaluate the argument through the
        // shared work-stack, then transform ([`Task::ApplyStringIntrinsic`]).
        let wk = self.interner.well_known();
        // Backlog 70: `ThisType<T>` is a context-only intrinsic marker. Preserve its
        // exact instantiation and operand for contextual object-literal extraction.
        if inst.base == wk.this_type {
            values.push(ty);
            return;
        }
        if inst.base == wk.omit_this_parameter {
            let Some(&(_, arg)) = inst.args.first() else {
                values.push(ty);
                return;
            };
            self.in_flight.insert(ty);
            tasks.push(Task::SetMemo(ty));
            tasks.push(Task::ApplyOmitThisParameter(ty));
            tasks.push(Task::Eval(arg));
            return;
        }
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

    /// Evaluate the standard `unknown extends ThisParameterType<T> ? T : …`
    /// guard for the trusted prelude marker. Only fully represented callable
    /// shapes are transformed; open/deferred inputs remain symbolic.
    pub(super) fn apply_omit_this_parameter(
        &mut self,
        id: TypeId,
        values: &mut Vec<TypeId>,
        error: TypeId,
        normalization: &dyn RelationNormalization,
    ) {
        let argument = values.pop().unwrap_or(error);
        let Some(inst) = self.interner.store().instantiation_type(id).cloned() else {
            values.push(argument);
            return;
        };
        let Some(&(param, _)) = inst.args.first() else {
            values.push(argument);
            return;
        };
        match self.interner.store().tag(argument) {
            TypeTag::TypeParam
            | TypeTag::Conditional
            | TypeTag::Instantiation
            | TypeTag::Mapped
            | TypeTag::Template
            | TypeTag::Keyof => {
                values.push(
                    self.interner
                        .intern_instantiation(inst.base, vec![(param, argument)]),
                );
            }
            TypeTag::Union => {
                let members = self
                    .interner
                    .store()
                    .union_members(argument)
                    .map(|members| members.to_vec())
                    .unwrap_or_default();
                let mut functions = Vec::with_capacity(members.len());
                for member in members {
                    let Some(function) = self.omit_this_parameter_signature(member) else {
                        values.push(argument);
                        return;
                    };
                    let Some(receiver) = function.receiver else {
                        values.push(argument);
                        return;
                    };
                    let effective_receiver = self.effective_receiver(&function, receiver);
                    match self.unknown_extends(effective_receiver, normalization) {
                        DemandOutcome::Ready(true) => {
                            values.push(argument);
                            return;
                        }
                        DemandOutcome::Ready(false) => {}
                        DemandOutcome::Exhausted(reason) => {
                            self.planned_exhaustion = Some(reason);
                            return;
                        }
                    }
                    functions.push(function);
                }
                let transformed = functions
                    .into_iter()
                    .map(|function| self.erase_this_parameter(function))
                    .collect();
                values.push(self.interner.union(transformed));
            }
            _ => {
                let Some(function) = self.omit_this_parameter_signature(argument) else {
                    values.push(argument);
                    return;
                };
                let Some(receiver) = function.receiver else {
                    values.push(argument);
                    return;
                };
                let effective_receiver = self.effective_receiver(&function, receiver);
                match self.unknown_extends(effective_receiver, normalization) {
                    DemandOutcome::Ready(true) => {
                        values.push(argument);
                        return;
                    }
                    DemandOutcome::Ready(false) => {}
                    DemandOutcome::Exhausted(reason) => {
                        self.planned_exhaustion = Some(reason);
                        return;
                    }
                }
                values.push(self.erase_this_parameter(function));
            }
        }
    }

    /// `ThisParameterType<T>` uses an overload's last represented signature.
    fn omit_this_parameter_signature(&self, ty: TypeId) -> Option<FunctionType> {
        match self.interner.store().tag(ty) {
            TypeTag::Function => self.interner.store().function_type(ty).cloned(),
            TypeTag::Object => self
                .interner
                .store()
                .object_type(ty)
                .and_then(|object| object.call_signatures.last())
                .and_then(|&signature| self.interner.store().function_type(signature))
                .cloned(),
            _ => None,
        }
    }

    fn unknown_extends(
        &self,
        receiver: TypeId,
        normalization: &dyn RelationNormalization,
    ) -> DemandOutcome<bool> {
        let mut relater = Relater::planned(
            self.interner.store(),
            self.interner.well_known(),
            RelationCache::new(),
            normalization,
        );
        match relater.is_assignable_outcome(self.interner.well_known().unknown, receiver) {
            RelationOutcome::Yes => DemandOutcome::Ready(true),
            RelationOutcome::No(_) => DemandOutcome::Ready(false),
            RelationOutcome::Exhausted(reason) => DemandOutcome::Exhausted(reason),
        }
    }

    /// The guard observes generic receivers after their binders are replaced by
    /// constraints or `unknown`; defaults never participate in this decision.
    fn effective_receiver(&mut self, function: &FunctionType, receiver: TypeId) -> TypeId {
        let substitutions = self.erasure_substitutions(function);
        substitute(self.interner, receiver, &substitutions)
    }

    /// Strip a represented receiver while preserving every positional parameter's
    /// optional/rest/default call shape. Generic signatures erase their binders to
    /// constraints (or `unknown`), intentionally ignoring defaults like lib.d.ts.
    fn erase_this_parameter(&mut self, function: FunctionType) -> TypeId {
        let substitutions = self.erasure_substitutions(&function);
        let params = function
            .params
            .into_iter()
            .map(|mut parameter| {
                parameter.ty = substitute(self.interner, parameter.ty, &substitutions);
                parameter
            })
            .collect();
        let ret = substitute(self.interner, function.ret, &substitutions);
        self.interner.intern_function(FunctionType {
            type_params: Vec::new(),
            receiver: None,
            params,
            ret,
        })
    }

    fn erasure_substitutions(&mut self, function: &FunctionType) -> FxHashMap<TypeParamId, TypeId> {
        let mut substitutions = FxHashMap::default();
        let unknown = self.interner.well_known().unknown;
        for type_param in &function.type_params {
            let value = type_param
                .constraint
                .map(|constraint| substitute(self.interner, constraint, &substitutions))
                .unwrap_or(unknown);
            substitutions.insert(type_param.id, value);
        }
        substitutions
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
        #[cfg(test)]
        measure_infer_rewrite(|measure| measure.top_level_runs += 1);
        let mut ctx = InferRewrite {
            fresh,
            in_progress: FxHashSet::default(),
            active: Vec::new(),
            cycle_tainted: FxHashSet::default(),
            memo: FxHashMap::default(),
        };
        let mut tasks = vec![InferRewriteTask::Visit(ty)];
        let mut values = Vec::new();

        while let Some(task) = tasks.pop() {
            match task {
                InferRewriteTask::Visit(ty) => {
                    #[cfg(test)]
                    measure_infer_rewrite(|measure| measure.visits += 1);
                    if let Some(&done) = ctx.memo.get(&ty) {
                        #[cfg(test)]
                        measure_infer_rewrite(|measure| measure.memo_hits += 1);
                        values.push(done);
                        continue;
                    }
                    if !ctx.in_progress.insert(ty) {
                        #[cfg(test)]
                        measure_infer_rewrite(|measure| measure.reentries += 1);
                        if let Some(cycle_start) =
                            ctx.active.iter().position(|&active| active == ty)
                        {
                            ctx.cycle_tainted
                                .extend(ctx.active[cycle_start..].iter().copied());
                        } else {
                            debug_assert!(false, "in-progress infer rewrite must be active");
                        }
                        values.push(ty);
                        continue;
                    }

                    ctx.active.push(ty);
                    let (frame, children) = self.infer_rewrite_frame(ty, ctx.fresh);
                    tasks.push(InferRewriteTask::Finish {
                        frame,
                        child_count: children.len(),
                    });
                    for child in children.into_iter().rev() {
                        tasks.push(InferRewriteTask::Visit(child));
                    }
                }
                InferRewriteTask::Finish { frame, child_count } => {
                    let ty = frame.ty();
                    let result = if let Some(start) = values.len().checked_sub(child_count) {
                        let children = values.split_off(start);
                        self.rebuild_infer_rewrite_frame(frame, &children)
                    } else {
                        debug_assert!(false, "infer rewrite frame is missing child results");
                        ty
                    };

                    let popped = ctx.active.pop();
                    debug_assert_eq!(popped, Some(ty));
                    let removed = ctx.in_progress.remove(&ty);
                    debug_assert!(removed);
                    if ctx.cycle_tainted.remove(&ty) {
                        #[cfg(test)]
                        measure_infer_rewrite(|measure| measure.tainted_identity_returns += 1);
                        values.push(ty);
                    } else {
                        ctx.memo.insert(ty, result);
                        #[cfg(test)]
                        measure_infer_rewrite(|measure| measure.memo_inserts += 1);
                        values.push(result);
                    }
                }
            }
        }

        values.pop().unwrap_or(ty)
    }

    fn infer_rewrite_frame(
        &self,
        ty: TypeId,
        fresh: &[TypeId],
    ) -> (InferRewriteFrame, Vec<TypeId>) {
        let identity = || (InferRewriteFrame::Identity { ty, result: ty }, Vec::new());
        match self.interner.store().tag(ty) {
            TypeTag::Infer => {
                let result = self
                    .interner
                    .store()
                    .infer_index(ty)
                    .and_then(|index| fresh.get(index as usize).copied())
                    .unwrap_or(ty);
                (InferRewriteFrame::Identity { ty, result }, Vec::new())
            }
            // A nested conditional rebinds its own infer indices; a mapped type / mapped
            // value placeholder (M26) carries no conditional infer binder — all opaque.
            TypeTag::Intrinsic
            | TypeTag::Literal
            | TypeTag::TypeParam
            | TypeTag::Conditional
            | TypeTag::Mapped
            | TypeTag::MappedValue
            | TypeTag::Declared => identity(),
            TypeTag::Object => {
                let Some(object) = self.interner.store().object_type(ty).cloned() else {
                    return identity();
                };
                let mut children = Vec::new();
                for property in &object.properties {
                    children.push(property.ty);
                    children.extend(property.write_ty);
                }
                children.extend(object.string_index);
                children.extend(object.number_index);
                children.extend(object.call_signatures.iter().copied());
                children.extend(object.construct_signatures.iter().copied());
                (InferRewriteFrame::Object { ty, object }, children)
            }
            TypeTag::Function => {
                let Some(function) = self.interner.store().function_type(ty).cloned() else {
                    return identity();
                };
                let mut children = Vec::new();
                for type_param in &function.type_params {
                    children.extend(type_param.constraint);
                    children.extend(type_param.default);
                }
                children.extend(function.receiver);
                children.extend(function.params.iter().map(|param| param.ty));
                children.push(function.ret);
                (InferRewriteFrame::Function { ty, function }, children)
            }
            TypeTag::Union => {
                let Some(members) = self.interner.store().union_members(ty).map(|m| m.to_vec())
                else {
                    return identity();
                };
                (
                    InferRewriteFrame::Union {
                        ty,
                        members: members.clone(),
                    },
                    members,
                )
            }
            TypeTag::Intersection => {
                let Some(members) = self
                    .interner
                    .store()
                    .intersection_members(ty)
                    .map(|m| m.to_vec())
                else {
                    return identity();
                };
                (
                    InferRewriteFrame::Intersection {
                        ty,
                        members: members.clone(),
                    },
                    members,
                )
            }
            TypeTag::Array => {
                let Some(element) = self
                    .interner
                    .store()
                    .array_type(ty)
                    .map(|array| array.element)
                else {
                    return identity();
                };
                (InferRewriteFrame::Array { ty, element }, vec![element])
            }
            TypeTag::Tuple => {
                let Some(tuple) = self.interner.store().tuple_type(ty).cloned() else {
                    return identity();
                };
                let mut children = tuple.elements.clone();
                children.extend(tuple.rest.map(|rest| rest.ty));
                (InferRewriteFrame::Tuple { ty, tuple }, children)
            }
            TypeTag::Readonly => {
                let Some(operand) = self.interner.store().readonly_operand(ty) else {
                    return identity();
                };
                (InferRewriteFrame::Readonly { ty, operand }, vec![operand])
            }
            TypeTag::Instantiation => {
                let Some(instantiation) = self.interner.store().instantiation_type(ty).cloned()
                else {
                    return identity();
                };
                let children = instantiation.args.iter().map(|(_, value)| *value).collect();
                (
                    InferRewriteFrame::Instantiation {
                        ty,
                        base: instantiation.base,
                        args: instantiation.args,
                    },
                    children,
                )
            }
            TypeTag::ClassInstance => {
                let Some(instance) = self.interner.store().class_instance_type(ty).cloned() else {
                    return identity();
                };
                let children = instance.args.clone();
                (
                    InferRewriteFrame::ClassInstance {
                        ty,
                        class: instance.class,
                        args: instance.args,
                    },
                    children,
                )
            }
            TypeTag::Template => {
                let Some(template) = self.interner.store().template_type(ty).cloned() else {
                    return identity();
                };
                let children = template.holes.clone();
                (InferRewriteFrame::Template { ty, template }, children)
            }
            TypeTag::Keyof => {
                let Some(operand) = self.interner.store().keyof_operand(ty) else {
                    return identity();
                };
                (InferRewriteFrame::Keyof { ty, operand }, vec![operand])
            }
            TypeTag::DeferredIndexedAccess => {
                let Some(access) = self
                    .interner
                    .store()
                    .deferred_indexed_access_type(ty)
                    .copied()
                else {
                    return identity();
                };
                (
                    InferRewriteFrame::DeferredIndexedAccess {
                        ty,
                        object: access.object,
                        index: access.index,
                    },
                    vec![access.object, access.index],
                )
            }
        }
    }

    fn rebuild_infer_rewrite_frame(
        &mut self,
        frame: InferRewriteFrame,
        children: &[TypeId],
    ) -> TypeId {
        match frame {
            InferRewriteFrame::Identity { result, .. } => result,
            InferRewriteFrame::Object { ty, object } => {
                let mut child_index = 0;
                let mut changed = false;
                let mut new = object.clone();
                for (property, new_property) in object.properties.iter().zip(&mut new.properties) {
                    let rewritten = infer_rewrite_child(children, &mut child_index, property.ty);
                    changed |= rewritten != property.ty;
                    new_property.ty = rewritten;
                    new_property.write_ty = property.write_ty.map(|write_ty| {
                        let rewritten = infer_rewrite_child(children, &mut child_index, write_ty);
                        changed |= rewritten != write_ty;
                        rewritten
                    });
                }
                new.string_index = object.string_index.map(|index_ty| {
                    let rewritten = infer_rewrite_child(children, &mut child_index, index_ty);
                    changed |= rewritten != index_ty;
                    rewritten
                });
                new.number_index = object.number_index.map(|index_ty| {
                    let rewritten = infer_rewrite_child(children, &mut child_index, index_ty);
                    changed |= rewritten != index_ty;
                    rewritten
                });
                new.call_signatures = object
                    .call_signatures
                    .iter()
                    .map(|&signature| {
                        let rewritten = infer_rewrite_child(children, &mut child_index, signature);
                        changed |= rewritten != signature;
                        rewritten
                    })
                    .collect();
                new.construct_signatures = object
                    .construct_signatures
                    .iter()
                    .map(|&signature| {
                        let rewritten = infer_rewrite_child(children, &mut child_index, signature);
                        changed |= rewritten != signature;
                        rewritten
                    })
                    .collect();
                if changed {
                    self.interner.intern_object(new)
                } else {
                    ty
                }
            }
            InferRewriteFrame::Function { ty, function } => {
                let mut child_index = 0;
                let mut changed = false;
                let mut new = function.clone();
                // Function-local binders retain their ids and order, but their
                // constraints/defaults are lexical children of the outer infer scope.
                for (original, rewritten) in function.type_params.iter().zip(&mut new.type_params) {
                    rewritten.constraint = original.constraint.map(|constraint| {
                        let value = infer_rewrite_child(children, &mut child_index, constraint);
                        changed |= value != constraint;
                        value
                    });
                    rewritten.default = original.default.map(|default| {
                        let value = infer_rewrite_child(children, &mut child_index, default);
                        changed |= value != default;
                        value
                    });
                }
                new.receiver = function.receiver.map(|receiver| {
                    let rewritten = infer_rewrite_child(children, &mut child_index, receiver);
                    changed |= rewritten != receiver;
                    rewritten
                });
                for (original, rewritten) in function.params.iter().zip(&mut new.params) {
                    let value = infer_rewrite_child(children, &mut child_index, original.ty);
                    changed |= value != original.ty;
                    rewritten.ty = value;
                }
                let ret = infer_rewrite_child(children, &mut child_index, function.ret);
                changed |= ret != function.ret;
                new.ret = ret;
                if changed {
                    self.interner.intern_function(new)
                } else {
                    ty
                }
            }
            InferRewriteFrame::Union { ty, members } => {
                let rewritten: Vec<TypeId> = members
                    .iter()
                    .enumerate()
                    .map(|(index, &member)| children.get(index).copied().unwrap_or(member))
                    .collect();
                if rewritten != members {
                    self.interner.union(rewritten)
                } else {
                    ty
                }
            }
            InferRewriteFrame::Intersection { ty, members } => {
                let rewritten: Vec<TypeId> = members
                    .iter()
                    .enumerate()
                    .map(|(index, &member)| children.get(index).copied().unwrap_or(member))
                    .collect();
                if rewritten != members {
                    self.interner.intersection(rewritten)
                } else {
                    ty
                }
            }
            InferRewriteFrame::Array { ty, element } => {
                let rewritten = children.first().copied().unwrap_or(element);
                if rewritten != element {
                    self.interner.intern_array(rewritten)
                } else {
                    ty
                }
            }
            InferRewriteFrame::Tuple { ty, tuple } => {
                let mut child_index = 0;
                let mut changed = false;
                let elements = tuple
                    .elements
                    .iter()
                    .map(|&element| {
                        let rewritten = infer_rewrite_child(children, &mut child_index, element);
                        changed |= rewritten != element;
                        rewritten
                    })
                    .collect();
                let rest = tuple.rest.map(|rest| {
                    let rewritten = infer_rewrite_child(children, &mut child_index, rest.ty);
                    changed |= rewritten != rest.ty;
                    TupleRestType {
                        ty: rewritten,
                        ..rest
                    }
                });
                if changed {
                    self.interner
                        .intern_tuple_type(TupleType { elements, rest })
                } else {
                    ty
                }
            }
            InferRewriteFrame::Readonly { ty, operand } => {
                let rewritten = children.first().copied().unwrap_or(operand);
                if rewritten != operand {
                    self.interner.intern_readonly(rewritten)
                } else {
                    ty
                }
            }
            InferRewriteFrame::Instantiation { ty, base, args } => {
                let rewritten: Vec<(TypeParamId, TypeId)> = args
                    .iter()
                    .enumerate()
                    .map(|(index, &(param, value))| {
                        (param, children.get(index).copied().unwrap_or(value))
                    })
                    .collect();
                if rewritten != args {
                    self.interner.intern_instantiation(base, rewritten)
                } else {
                    ty
                }
            }
            InferRewriteFrame::ClassInstance { ty, class, args } => {
                let rewritten = args
                    .iter()
                    .enumerate()
                    .map(|(index, &arg)| children.get(index).copied().unwrap_or(arg))
                    .collect::<Vec<_>>();
                if rewritten != args {
                    self.interner.intern_class_instance(class, rewritten)
                } else {
                    ty
                }
            }
            InferRewriteFrame::Template { ty, template } => {
                let holes: Vec<TypeId> = template
                    .holes
                    .iter()
                    .enumerate()
                    .map(|(index, &hole)| children.get(index).copied().unwrap_or(hole))
                    .collect();
                if holes != template.holes {
                    self.interner.intern_template(TemplateType {
                        texts: template.texts,
                        holes,
                    })
                } else {
                    ty
                }
            }
            InferRewriteFrame::Keyof { ty, operand } => {
                let rewritten = children.first().copied().unwrap_or(operand);
                if rewritten != operand {
                    self.interner.intern_keyof(rewritten)
                } else {
                    ty
                }
            }
            InferRewriteFrame::DeferredIndexedAccess { ty, object, index } => {
                let rewritten_object = children.first().copied().unwrap_or(object);
                let rewritten_index = children.get(1).copied().unwrap_or(index);
                if rewritten_object != object || rewritten_index != index {
                    self.interner
                        .intern_deferred_indexed_access(rewritten_object, rewritten_index)
                } else {
                    ty
                }
            }
        }
    }
}

fn infer_rewrite_child(children: &[TypeId], index: &mut usize, original: TypeId) -> TypeId {
    let rewritten = children.get(*index).copied().unwrap_or(original);
    *index += 1;
    rewritten
}

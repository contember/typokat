//! Conditional-type evaluation (M25; architecture §7).
//! Concrete conditionals split distributive unions, run `extends` through the
//! relater plus conditional-mode infer, then substitute matched `infer` candidates.
//! Exhausted or in-flight results are never memoized; the explicit work-stack
//! avoids host-stack overflow and reports `TK2589` on budget exhaustion.

use super::context::Pass;
use crate::check::infer::infer_from_types_for_conditional;
use crate::diagnostics::Diagnostic;
use crate::relate::Relater;
use crate::span::Span;
use crate::types::repr::{
    ConditionalType, FunctionType, LiteralValue, MappedType, ModifierOp, ObjectType, ParameterType,
    PropertyType, TemplateType, TypeParamId, TypeTag,
};
use crate::types::store::TypeId;
use crate::types::{substitute, Interner};
use rustc_hash::{FxHashMap, FxHashSet};

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Evaluate a lowered type at a value-position demand site. Concrete
    /// conditionals, lazy instantiations, and unions of those resolve; deferred
    /// conditionals and other types are unchanged. Exhaustion reports `TK2589` at
    /// the demand span, a documented tsc span divergence.
    pub(in crate::check::checker) fn evaluate_type(&mut self, ty: TypeId, span: Span) -> TypeId {
        if !matches!(
            self.interner.store().tag(ty),
            TypeTag::Conditional
                | TypeTag::Instantiation
                | TypeTag::Union
                | TypeTag::Mapped
                | TypeTag::Template
                | TypeTag::Keyof
        ) {
            return ty;
        }
        let result;
        let exhausted;
        {
            let mut ev = ConditionalEvaluator::new(
                &mut *self.interner,
                &mut self.next_type_param,
                &mut self.cond_memo,
                DEFAULT_STEP_BUDGET,
            );
            result = ev.evaluate(ty);
            exhausted = ev.exhausted;
        }
        if exhausted {
            self.diagnostics.push(Diagnostic::excessively_deep(span));
        }
        result
    }
}

pub(in crate::check) struct InferenceConstraintEvaluation {
    pub(in crate::check) result: TypeId,
    pub(in crate::check) exhausted: bool,
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
    }
}

struct InferenceConstraintEvaluator<'a> {
    interner: &'a mut Interner,
    next_type_param: &'a mut u32,
    memo: &'a mut FxHashMap<TypeId, TypeId>,
    in_progress: FxHashSet<TypeId>,
    exhausted: bool,
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
        {
            let mut ev = ConditionalEvaluator::new(
                self.interner,
                self.next_type_param,
                self.memo,
                DEFAULT_STEP_BUDGET,
            );
            result = ev.evaluate(ty);
            exhausted = ev.exhausted;
        }
        if exhausted {
            self.exhausted = true;
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
            self.interner.intern_function(FunctionType { params, ret })
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
        let Some(elements) = self
            .interner
            .store()
            .tuple_type(ty)
            .map(|tuple| tuple.elements.clone())
        else {
            return ty;
        };
        self.in_progress.insert(ty);
        let mut changed = false;
        let elements: Vec<TypeId> = elements
            .into_iter()
            .map(|element| {
                let new_ty = self.evaluate(element);
                changed |= new_ty != element;
                new_ty
            })
            .collect();
        self.in_progress.remove(&ty);

        if changed {
            self.interner.intern_tuple(elements)
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

/// The default per-root instantiation step budget. Of tsc's tail-iteration order of
/// magnitude (§7.4); enough for the corpus's 16-deep recursion, tight enough to trip a
/// genuinely-infinite alias. The 10k-deep witness test raises it (proving the work-stack
/// prevents native-stack overflow *independent* of this policy).
pub const DEFAULT_STEP_BUDGET: u32 = 1000;

/// One unit of pending work on the explicit evaluation stack (no host recursion).
enum Task {
    /// Evaluate this type, pushing its result onto the value stack.
    Eval(TypeId),
    /// Pop the top value, commit it as `memo[id]` (unless exhausted), pop `id` from the
    /// in-flight set, and push the value back.
    SetMemo(TypeId),
    /// Pop `n` results, union them, push the union.
    BuildUnion(usize),
    /// M26: the mapped type `id`'s key source is on the value stack (freshly evaluated).
    /// Pop it, derive each output property's metadata + pre-evaluation value type, then
    /// schedule the per-property value evaluations and a [`Task::BuildMappedObject`].
    AssembleMapped(TypeId),
    /// M26: pop one evaluated value per recorded property (in order), assemble the
    /// result object type, and push it. The metadata carries each property's name +
    /// resolved optional/readonly flags.
    BuildMappedObject(Vec<MappedProp>),
    /// M28: the deferred keyof `id`'s operand is on the value stack (freshly
    /// evaluated). Pop it and resolve: an object operand keys through the SHARED
    /// keyof computation; error/any degrades to the error type; anything else stays a
    /// (rebuilt) deferred node.
    BuildKeyof(TypeId),
    /// M28: the template `id`'s evaluable holes are on the value stack (freshly
    /// evaluated, one per hole in order). Pop them and finish construction inline —
    /// collapse / cartesian union / `never` / symbolic — WITHOUT re-scheduling (a
    /// still-deferred hole must not loop back through [`Task::Eval`]).
    FinishTemplate(TypeId),
    /// M28: the distributive instantiation `id`'s check argument is on the value
    /// stack (freshly evaluated — it was an evaluable node, e.g. `keyof P` or a nested
    /// `Exclude<…>`). Pop it, re-derive the per-member conditionals from the evaluated
    /// argument, and schedule their evaluation + the result union.
    ExpandDistributive(TypeId),
    /// M28: the string-intrinsic instantiation `id`'s argument is on the value stack
    /// (freshly evaluated). Pop it and apply the intrinsic: a string literal
    /// transforms, a union distributes per member, anything else stays a (rebuilt)
    /// symbolic instantiation.
    ApplyStringIntrinsic(TypeId),
    /// M28: pop pre-evaluated check/extends operands and decide the branch; see
    /// [`ConditionalEvaluator::operand_undecidable`] for the conservative `No` gate.
    DecideConditional(TypeId),
}

/// The classification of a template hole for construction (M27) — see
/// [`ConditionalEvaluator::hole_parts`].
enum HolePart {
    /// A `never` hole: the whole template collapses to `never`.
    Never,
    /// A non-literal hole (string/number intrinsic, free parameter, `infer`, …): the
    /// template stays a symbolic pattern.
    NonLiteral,
    /// A literal (or union-of-literals) hole: the ordered string parts it contributes to
    /// the cartesian product.
    Literals(Vec<String>),
}

/// The string a literal value contributes to a constructed template (M27): a string
/// literal is its value, a number is JS-`String(n)`, a boolean is `"false"`/`"true"`.
fn literal_to_string(lit: &LiteralValue) -> String {
    match lit {
        LiteralValue::String(s) => s.clone(),
        LiteralValue::Number(n) => crate::types::repr::number_to_string(*n),
        LiteralValue::Boolean(b) => if *b { "true" } else { "false" }.to_string(),
    }
}

/// One resolved output property of a mapped type (M26): its name and the
/// modifier-arithmetic result flags. The property's value type is evaluated separately
/// (routed through the work-stack) and paired back by position in
/// [`Task::BuildMappedObject`].
struct MappedProp {
    name: String,
    optional: bool,
    readonly: bool,
    /// `-?` over an **optional** source member also strips `undefined` from the
    /// property's **evaluated** value type (tsc `Required` semantics, probed 6.0.3:
    /// even a template-re-added `| undefined` is removed; a result that is EXACTLY
    /// `undefined` maps to `never`; a non-optional source member never strips).
    strip_undefined: bool,
}

/// The conditional-type evaluator. Standalone (borrows only an [`Interner`], a
/// type-parameter counter, and a shared memo) so it is unit-testable without a full
/// checker pass. Constructed per demand-site evaluation via
/// [`crate::check::checker::Pass::evaluate_type`].
pub struct ConditionalEvaluator<'a> {
    interner: &'a mut Interner,
    /// The module-wide type-parameter counter (advanced when freshening `infer`
    /// binders to transient parameters).
    next_type_param: &'a mut u32,
    /// Durable evaluation memo `substituted-conditional/instantiation id → result`.
    memo: &'a mut FxHashMap<TypeId, TypeId>,
    /// Per-run concreteness cache (`id → contains no free type parameter`), so the deep
    /// check types of a recursive descent are classified in `O(total nodes)`, not
    /// `O(depth²)`.
    concrete: FxHashMap<TypeId, bool>,
    /// The ids currently in flight (scheduled but not yet memoized). Re-entering one is a
    /// genuine cycle → the error type (never memoized).
    in_flight: FxHashSet<TypeId>,
    budget: u32,
    steps: u32,
    /// Set once the per-root budget is exhausted; the caller reports `TK2589`.
    pub exhausted: bool,
}

impl<'a> ConditionalEvaluator<'a> {
    pub fn new(
        interner: &'a mut Interner,
        next_type_param: &'a mut u32,
        memo: &'a mut FxHashMap<TypeId, TypeId>,
        budget: u32,
    ) -> Self {
        ConditionalEvaluator {
            interner,
            next_type_param,
            memo,
            concrete: FxHashMap::default(),
            in_flight: FxHashSet::default(),
            budget,
            steps: 0,
            exhausted: false,
        }
    }

    /// Evaluate `root`, resolving every concrete conditional / lazy instantiation it
    /// directly denotes (and, for a union, its members) to a result type. A deferred
    /// conditional (free check), or any non-conditional type, is returned unchanged.
    pub fn evaluate(&mut self, root: TypeId) -> TypeId {
        let mut tasks: Vec<Task> = vec![Task::Eval(root)];
        let mut values: Vec<TypeId> = Vec::new();
        let error = self.interner.well_known().error;

        while let Some(task) = tasks.pop() {
            match task {
                Task::SetMemo(id) => {
                    let value = values.pop().unwrap_or(error);
                    // Never durably memoize a result reached under budget exhaustion
                    // (provisional) — invariants §1 mirror.
                    if !self.exhausted {
                        self.memo.insert(id, value);
                    }
                    self.in_flight.remove(&id);
                    values.push(value);
                }
                Task::BuildUnion(n) => {
                    let start = values.len().saturating_sub(n);
                    let members: Vec<TypeId> = values.split_off(start);
                    values.push(self.interner.union(members));
                }
                Task::AssembleMapped(id) => {
                    self.assemble_mapped(id, &mut tasks, &mut values, error);
                }
                Task::BuildMappedObject(meta) => {
                    let start = values.len().saturating_sub(meta.len());
                    let vals: Vec<TypeId> = values.split_off(start);
                    values.push(self.build_mapped_object(&meta, &vals));
                }
                Task::BuildKeyof(id) => {
                    self.build_keyof(id, &mut values, error);
                }
                Task::FinishTemplate(id) => {
                    self.finish_template(id, &mut values, error);
                }
                Task::ExpandDistributive(id) => {
                    self.expand_distributive(id, &mut tasks, &mut values);
                }
                Task::ApplyStringIntrinsic(id) => {
                    self.apply_string_intrinsic(id, &mut values, error);
                }
                Task::DecideConditional(id) => {
                    self.decide_conditional(id, &mut tasks, &mut values, error);
                }
                Task::Eval(ty) => {
                    if let Some(&cached) = self.memo.get(&ty) {
                        values.push(cached);
                        continue;
                    }
                    match self.interner.store().tag(ty) {
                        TypeTag::Conditional => {
                            self.eval_conditional(ty, &mut tasks, &mut values, error)
                        }
                        TypeTag::Instantiation => {
                            self.eval_instantiation(ty, &mut tasks, &mut values, error)
                        }
                        TypeTag::Union => self.eval_union(ty, &mut tasks, &mut values),
                        TypeTag::Mapped => self.eval_mapped(ty, &mut tasks, &mut values, error),
                        TypeTag::Template => self.eval_template(ty, &mut tasks, &mut values, error),
                        TypeTag::Keyof => self.eval_keyof(ty, &mut tasks, &mut values, error),
                        // Any other type is already a value.
                        _ => values.push(ty),
                    }
                }
            }
        }

        values.pop().unwrap_or(error)
    }

    /// Schedule the evaluation of a conditional `ty`. A deferred (free check) conditional
    /// is a value; a concrete one runs the extends test and schedules its taken branch
    /// (a tail step, so the deep recursive descent is a loop, not host recursion).
    fn eval_conditional(
        &mut self,
        ty: TypeId,
        tasks: &mut Vec<Task>,
        values: &mut Vec<TypeId>,
        error: TypeId,
    ) {
        let Some(cond) = self.interner.store().conditional_type(ty).copied() else {
            values.push(ty);
            return;
        };
        // Poisoned (cross-binder nested infer — backlog 26 stopgap): NEVER evaluates.
        // It stays a deferred node under the conservative relation rules.
        if cond.poisoned {
            values.push(ty);
            return;
        }
        // Deferred: a free declaration type parameter in the check leaves the whole
        // conditional an ordinary interned type (WU3 handles its relations).
        if !self.is_concrete(cond.check) {
            values.push(ty);
            return;
        }
        // A genuine self-cycle (the same id re-entered while in flight) → error, and it
        // is NOT memoized.
        if self.in_flight.contains(&ty) || self.exhausted {
            values.push(error);
            return;
        }
        self.steps += 1;
        if self.steps > self.budget {
            self.exhausted = true;
            values.push(error);
            return;
        }
        self.in_flight.insert(ty);
        // Demand-evaluate top-level pending operands before the extends test; relating
        // the raw nodes would turn "not proven" into a false-branch choice.
        if self.arg_needs_pre_eval(cond.check) || self.arg_needs_pre_eval(cond.extends_ty) {
            tasks.push(Task::SetMemo(ty));
            tasks.push(Task::DecideConditional(ty));
            tasks.push(Task::Eval(cond.check));
            tasks.push(Task::Eval(cond.extends_ty));
            return;
        }
        // The deep undecidable gate below remains the canonical no-false safeguard.
        let (matched, true_final) = self.run_extends_test(&cond);
        // The result memoizes under this id once it resolves (tail step — a chain of
        // conditionals is a loop here).
        tasks.push(Task::SetMemo(ty));
        if matched {
            tasks.push(Task::Eval(true_final));
        } else if self.operand_undecidable(cond.check) || self.operand_undecidable(cond.extends_ty)
        {
            // Cannot prove the relation either way — stay deferred (SetMemo commits
            // ty → ty, idempotent).
            values.push(ty);
        } else {
            tasks.push(Task::Eval(cond.false_branch));
        }
    }

    /// Decide a conditional whose check/extends operands were pre-evaluated (M28
    /// review round 2). The enclosing [`Task::SetMemo`] commits the result under the
    /// original node id; [`ConditionalEvaluator::operand_undecidable`] owns the
    /// no-false-on-undecidable invariant.
    fn decide_conditional(
        &mut self,
        ty: TypeId,
        tasks: &mut Vec<Task>,
        values: &mut Vec<TypeId>,
        error: TypeId,
    ) {
        // Push order in `eval_conditional` was [.., Eval(check), Eval(extends)], so the
        // check evaluated LAST and sits on top.
        let check = values.pop().unwrap_or(error);
        let extends_ty = values.pop().unwrap_or(error);
        let Some(cond) = self.interner.store().conditional_type(ty).copied() else {
            values.push(ty);
            return;
        };
        let evaluated = ConditionalType {
            check,
            extends_ty,
            ..cond
        };
        let (matched, true_final) = self.run_extends_test(&evaluated);
        if matched {
            tasks.push(Task::Eval(true_final));
            return;
        }
        if self.operand_undecidable(check) || self.operand_undecidable(extends_ty) {
            // Cannot prove the relation either way — never force the false branch.
            // The node stays its own deferred value (SetMemo commits ty → ty,
            // idempotent, mirroring the deferred mapped/keyof discipline).
            values.push(ty);
            return;
        }
        tasks.push(Task::Eval(cond.false_branch));
    }

    /// Whether a conditional operand carries an unevaluable deferred node — a symbolic
    /// instantiation, a deferred keyof / mapped / conditional — at **any structural
    /// depth** (M28 review round 3): through object property types, index-signature
    /// values, call/construct signatures, function parameters + returns, tuple and
    /// array elements, and union members. The relation answers `No` conservatively
    /// through such a nested node (`{ v: keyof T } extends { v: "a" }` fails at depth
    /// without proof), so an unmatched conditional whose operand trips this walk must
    /// stay deferred — never the false branch. Deep PRE-evaluation is deliberately NOT
    /// performed (leader arbitration: tsc's resolution of these shapes is mixed —
    /// eager-false for some, evaluated for others — chasing it is backlog 36; the
    /// deferred verdict is the FN-free direction across the whole arbitration table).
    ///
    /// **Template patterns are excluded** (the round-2 recorded deviation): the M27
    /// anchored-matching model genuinely decides them, and gating them would regress
    /// the pattern-extends fixtures. Intrinsics, literals, type parameters, `infer`
    /// binders, and the mapped-value placeholder are decidable leaves. Iterative with
    /// a visited set so recursive interned types terminate.
    fn operand_undecidable(&self, operand: TypeId) -> bool {
        let store = self.interner.store();
        let mut stack = vec![operand];
        let mut visited: FxHashSet<TypeId> = FxHashSet::default();
        while let Some(t) = stack.pop() {
            if !visited.insert(t) {
                continue;
            }
            match store.tag(t) {
                TypeTag::Conditional
                | TypeTag::Instantiation
                | TypeTag::Mapped
                | TypeTag::Keyof => return true,
                TypeTag::Object => {
                    if let Some(object) = store.object_type(t) {
                        stack.extend(object.properties.iter().map(|p| p.ty));
                        stack.extend(object.string_index);
                        stack.extend(object.number_index);
                        stack.extend(object.call_signatures.iter().copied());
                        stack.extend(object.construct_signatures.iter().copied());
                    }
                }
                TypeTag::Function => {
                    if let Some(f) = store.function_type(t) {
                        stack.extend(f.params.iter().map(|p| p.ty));
                        stack.push(f.ret);
                    }
                }
                TypeTag::Union => {
                    if let Some(members) = store.union_members(t) {
                        stack.extend(members.iter().copied());
                    }
                }
                // M31: an intersection is undecidable iff a member is — descend.
                TypeTag::Intersection => {
                    if let Some(members) = store.intersection_members(t) {
                        stack.extend(members.iter().copied());
                    }
                }
                TypeTag::Array => {
                    if let Some(a) = store.array_type(t) {
                        stack.push(a.element);
                    }
                }
                TypeTag::Tuple => {
                    if let Some(tup) = store.tuple_type(t) {
                        stack.extend(tup.elements.iter().copied());
                    }
                }
                TypeTag::Readonly => {
                    if let Some(operand) = store.readonly_operand(t) {
                        stack.push(operand);
                    }
                }
                // Decidable leaves — see the doc above (templates deliberately
                // excluded per the round-2 deviation).
                TypeTag::Template
                | TypeTag::Intrinsic
                | TypeTag::Literal
                | TypeTag::TypeParam
                | TypeTag::Infer
                | TypeTag::MappedValue => {}
            }
        }
        false
    }

    /// Schedule the evaluation of a lazy instantiation `substitute(base, args)`. When
    /// `base` is a **distributive** conditional and the check argument distributes
    /// (union / `never` / `boolean`), build one concrete per-member conditional and union
    /// their results; otherwise a single concrete conditional.
    fn eval_instantiation(
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
        if self.in_flight.contains(&ty) || self.exhausted {
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
    fn distributive_check_arg(
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
    fn arg_needs_pre_eval(&self, arg: TypeId) -> bool {
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
    fn expand_distributive(&mut self, ty: TypeId, tasks: &mut Vec<Task>, values: &mut Vec<TypeId>) {
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

    /// Apply a string intrinsic to its (freshly evaluated) argument (M28/WU3): a string
    /// literal transforms (Rust `to_uppercase`/`to_lowercase`; Capitalize/Uncapitalize
    /// touch the first char only); a union distributes per member; an error/any
    /// argument degrades to the error type (M22); anything else (a template pattern,
    /// the `string` intrinsic, a free parameter) stays a **symbolic** instantiation —
    /// rebuilt over the evaluated argument — relating conservatively (identical-node;
    /// → `string`).
    fn apply_string_intrinsic(&mut self, ty: TypeId, values: &mut Vec<TypeId>, error: TypeId) {
        let arg = values.pop().unwrap_or(error);
        let Some(inst) = self.interner.store().instantiation_type(ty).cloned() else {
            values.push(ty);
            return;
        };
        let wk = self.interner.well_known();
        if arg == wk.error || arg == wk.any {
            values.push(error);
            return;
        }
        let Some(&(param, _)) = inst.args.first() else {
            values.push(ty);
            return;
        };
        let members: Vec<TypeId> = match self.interner.store().union_members(arg) {
            Some(members) => members.to_vec(),
            None => vec![arg],
        };
        let mut results: Vec<TypeId> = Vec::with_capacity(members.len());
        for member in members {
            let transformed =
                self.interner
                    .store()
                    .literal_value(member)
                    .and_then(|lit| match lit {
                        LiteralValue::String(s) => Some(transform_string_intrinsic(
                            &self.interner.well_known(),
                            inst.base,
                            s,
                        )),
                        _ => None,
                    });
            match transformed {
                Some(out) => results.push(self.interner.intern_literal(LiteralValue::String(out))),
                // A non-string-literal member stays a symbolic per-member application.
                // Hash-consing makes a rebuild over the unchanged single argument THE
                // original node, so identical-node relations stay total.
                None => results.push(
                    self.interner
                        .intern_instantiation(inst.base, vec![(param, member)]),
                ),
            }
        }
        // A 1-member list collapses through `union` to that member.
        let result = self.interner.union(results);
        values.push(result);
    }

    /// Produce the concrete conditional(s) an instantiation expands to. A distributive
    /// base whose check argument is a union / `never` / `boolean` yields one plain
    /// substitution per distributed member; anything else yields a single plain
    /// substitution.
    fn expand_instantiation(
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
    fn substitute_member(
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
    fn distribute_members(&mut self, ty: TypeId) -> Vec<TypeId> {
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
    fn eval_union(&mut self, ty: TypeId, tasks: &mut Vec<Task>, values: &mut Vec<TypeId>) {
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

    /// Schedule mapped-type evaluation. Free key sources defer conservatively;
    /// concrete key sources evaluate as tail steps before [`Task::AssembleMapped`]
    /// derives the output properties.
    fn eval_mapped(
        &mut self,
        ty: TypeId,
        tasks: &mut Vec<Task>,
        values: &mut Vec<TypeId>,
        error: TypeId,
    ) {
        let Some(mapped) = self.interner.store().mapped_type(ty).copied() else {
            values.push(ty);
            return;
        };
        // Deferred: a free declaration type parameter (in the key source or value
        // template) leaves the whole mapped type an ordinary interned (deferred) node.
        if !self.is_concrete(ty) {
            values.push(ty);
            return;
        }
        if self.in_flight.contains(&ty) || self.exhausted {
            values.push(error);
            return;
        }
        self.steps += 1;
        if self.steps > self.budget {
            self.exhausted = true;
            values.push(error);
            return;
        }
        self.in_flight.insert(ty);
        // The result IS the assembled object; memoize this id to it once the key source
        // resolves and the properties are built (tail steps). M28: a captured modifiers
        // source evaluates through the same stack (so `Pick<Partial<P>, …>` composes);
        // the tasks pop in push-reverse order, so `assemble_mapped` pops the key source
        // first, then the modifiers source.
        tasks.push(Task::SetMemo(ty));
        tasks.push(Task::AssembleMapped(ty));
        tasks.push(Task::Eval(mapped.key_source));
        if let Some(ms) = mapped.modifiers_source {
            tasks.push(Task::Eval(ms));
        }
    }

    /// Schedule the evaluation of a deferred `keyof` (M28). A node whose operand still
    /// contains a free declaration type parameter stays deferred (its own value,
    /// conservative relations); a concrete one first evaluates its operand through the
    /// shared work-stack (the operand may itself be an instantiation / mapped /
    /// conditional — `keyof Omit<P, "a">`), then [`Task::BuildKeyof`] resolves it
    /// through the SAME keyof computation the eager path uses ([`keyof_of_object`] —
    /// single source of truth).
    fn eval_keyof(
        &mut self,
        ty: TypeId,
        tasks: &mut Vec<Task>,
        values: &mut Vec<TypeId>,
        error: TypeId,
    ) {
        let Some(operand) = self.interner.store().keyof_operand(ty) else {
            values.push(ty);
            return;
        };
        // Deferred: a free declaration type parameter in the operand leaves the node
        // an ordinary interned (deferred) type.
        if !self.is_concrete(ty) {
            values.push(ty);
            return;
        }
        if self.in_flight.contains(&ty) || self.exhausted {
            values.push(error);
            return;
        }
        self.steps += 1;
        if self.steps > self.budget {
            self.exhausted = true;
            values.push(error);
            return;
        }
        self.in_flight.insert(ty);
        tasks.push(Task::SetMemo(ty));
        tasks.push(Task::BuildKeyof(ty));
        tasks.push(Task::Eval(operand));
    }

    /// Resolve a deferred `keyof` whose operand has been evaluated (M28 — the operand
    /// result is on top of the value stack). A concrete object or union-of-objects
    /// operand keys through the shared [`keyof_of_type`] computation; an error/any
    /// operand degrades to the error type (M22 cascade suppression); any other shape
    /// stays a deferred value (rebuilt over the evaluated operand), never a permissive
    /// fallback.
    fn build_keyof(&mut self, ty: TypeId, values: &mut Vec<TypeId>, error: TypeId) {
        let operand = values.pop().unwrap_or(error);
        let wk = self.interner.well_known();
        if operand == wk.error || operand == wk.any {
            values.push(error);
            return;
        }
        if let Some(keys) = keyof_of_type(self.interner, operand) {
            values.push(keys);
            return;
        }
        let node = if self.interner.store().keyof_operand(ty) == Some(operand) {
            ty
        } else {
            self.interner.intern_keyof(operand)
        };
        values.push(node);
    }

    /// **Construct** a template literal type (M27). When every hole is a string / number
    /// / boolean literal (or a union thereof) the template **collapses**: a single
    /// combination to a string literal, several to the cartesian-product **union**
    /// (canonicalized by `Interner::union`). A `never` hole short-circuits the whole
    /// template to `never`; a `boolean` hole expands to `"false" | "true"` before the
    /// product. A **non-literal** hole (`string`/`number` intrinsic, a free declaration
    /// type parameter, an `infer` binder, or any still-symbolic type) leaves the template
    /// a **symbolic pattern** — returned unchanged. The cartesian product iterates under
    /// the shared per-root step budget, so a combinatorial blow-up trips `TK2589` (via
    /// `exhausted`), never OOM. The result is committed through [`Task::SetMemo`], which
    /// refuses to commit under an exhausted budget — so a hole that resolved to error
    /// only because an unrelated earlier member drained the budget never poisons the
    /// pass-wide memo (backlog 55). A symbolic survivor memoizes to itself, idempotent.
    fn eval_template(
        &mut self,
        ty: TypeId,
        tasks: &mut Vec<Task>,
        values: &mut Vec<TypeId>,
        error: TypeId,
    ) {
        let Some(template) = self.interner.store().template_type(ty).cloned() else {
            values.push(ty);
            return;
        };
        // A self-cycle, or a result reached under an already-exhausted budget, is the
        // error type and is NOT memoized (backlog 55 — the template path formerly
        // bypassed this gate and poisoned the shared memo; mirrors the other node
        // kinds and invariants §1).
        if self.in_flight.contains(&ty) || self.exhausted {
            values.push(error);
            return;
        }
        self.steps += 1;
        if self.steps > self.budget {
            self.exhausted = true;
            values.push(error);
            return;
        }
        self.in_flight.insert(ty);

        // M28: a hole may itself be a pending type-level computation (a substituted
        // string-intrinsic instantiation — the `Greet` composition — a conditional, a
        // keyof, …). Evaluate such holes through the shared work-stack FIRST, then
        // finish construction inline ([`Task::FinishTemplate`] never re-schedules, so
        // a hole that stays deferred cannot loop). The enclosing [`Task::SetMemo`]
        // commits the result under `ty` (skipped when exhausted — backlog 55).
        let needs_eval = template.holes.iter().any(|&h| self.arg_needs_pre_eval(h));
        if needs_eval {
            tasks.push(Task::SetMemo(ty));
            tasks.push(Task::FinishTemplate(ty));
            for &hole in template.holes.iter().rev() {
                tasks.push(Task::Eval(hole));
            }
            return;
        }

        tasks.push(Task::SetMemo(ty));
        let holes = template.holes.clone();
        self.finish_template_with_holes(ty, &template, holes, values, error);
    }

    /// Finish a template whose evaluable holes were pre-evaluated (M28): pop one value
    /// per hole (in order) and construct inline.
    fn finish_template(&mut self, ty: TypeId, values: &mut Vec<TypeId>, error: TypeId) {
        let Some(template) = self.interner.store().template_type(ty).cloned() else {
            values.push(ty);
            return;
        };
        let start = values.len().saturating_sub(template.holes.len());
        let holes: Vec<TypeId> = values.split_off(start);
        self.finish_template_with_holes(ty, &template, holes, values, error);
    }

    /// The template construction core (M27, factored for M28's hole pre-evaluation):
    /// classify the (possibly re-evaluated) `holes`, then collapse / short-circuit /
    /// stay symbolic exactly as before. Pushes exactly one result value; the enclosing
    /// [`Task::SetMemo`] (scheduled by [`Self::eval_template`]) commits it under the
    /// ORIGINAL node id `ty` — but never under an exhausted budget (backlog 55). A
    /// symbolic survivor whose holes changed re-interns over the resolved holes.
    fn finish_template_with_holes(
        &mut self,
        ty: TypeId,
        template: &TemplateType,
        holes: Vec<TypeId>,
        values: &mut Vec<TypeId>,
        error: TypeId,
    ) {
        let wk = self.interner.well_known();

        // M22 discipline: an error-typed hole (an unresolved name upstream) degrades the
        // whole template to the error type so cascades stay suppressed — mirroring
        // `assemble_mapped`'s error/any key-source handling. Commit via the enclosing
        // `Task::SetMemo` (backlog 55 — never a direct insert that ignores exhaustion).
        if holes.contains(&wk.error) {
            values.push(error);
            return;
        }

        // Classify each hole; a `never` hole makes the whole template `never`, a
        // non-literal hole keeps it symbolic, otherwise it is a cartesian factor.
        let mut factors: Vec<Vec<String>> = Vec::with_capacity(holes.len());
        for &hole in &holes {
            match self.hole_parts(hole) {
                HolePart::Never => {
                    values.push(wk.never);
                    return;
                }
                HolePart::NonLiteral => {
                    // A symbolic pattern (string/number intrinsic, free param, a
                    // still-symbolic intrinsic application, …) — keep the node
                    // symbolic, un-memoized (idempotent). Holes that DID evaluate are
                    // baked in (re-interned) so relations see the resolved form.
                    let node = if holes == template.holes {
                        ty
                    } else {
                        self.interner.intern_template(TemplateType {
                            texts: template.texts.clone(),
                            holes,
                        })
                    };
                    values.push(node);
                    return;
                }
                HolePart::Literals(parts) => factors.push(parts),
            }
        }

        // All-literal holes: build the cartesian product of text + hole combinations,
        // metering each combination against the shared step budget.
        let empty = String::new();
        let mut acc: Vec<String> = vec![template.texts.first().cloned().unwrap_or_default()];
        for (i, factor) in factors.iter().enumerate() {
            let sep = template.texts.get(i + 1).unwrap_or(&empty);
            let mut next: Vec<String> = Vec::with_capacity(acc.len().saturating_mul(factor.len()));
            for prefix in &acc {
                for part in factor {
                    self.steps += 1;
                    if self.steps > self.budget {
                        self.exhausted = true;
                        values.push(error);
                        return;
                    }
                    next.push(format!("{prefix}{part}{sep}"));
                }
            }
            acc = next;
        }

        let members: Vec<TypeId> = acc
            .into_iter()
            .map(|s| self.interner.intern_literal(LiteralValue::String(s)))
            .collect();
        let result = self.interner.union(members);
        values.push(result);
    }

    /// Classify a template hole for construction. Literal string/number/boolean
    /// inputs feed the cartesian product, `never` short-circuits, and non-literal
    /// inputs leave the template symbolic.
    fn hole_parts(&self, hole: TypeId) -> HolePart {
        let wk = self.interner.well_known();
        let store = self.interner.store();
        if hole == wk.never {
            return HolePart::Never;
        }
        if hole == wk.boolean {
            return HolePart::Literals(vec!["false".to_string(), "true".to_string()]);
        }
        if let Some(lit) = store.literal_value(hole) {
            return HolePart::Literals(vec![literal_to_string(lit)]);
        }
        if let Some(members) = store.union_members(hole) {
            // Every member must itself be constructible (a `never` member cannot occur —
            // the interner drops it from a union). A non-literal member keeps the whole
            // template symbolic.
            let mut parts: Vec<String> = Vec::with_capacity(members.len());
            for &member in members {
                match self.hole_parts(member) {
                    HolePart::Literals(sub) => parts.extend(sub),
                    HolePart::Never => return HolePart::NonLiteral,
                    HolePart::NonLiteral => return HolePart::NonLiteral,
                }
            }
            return HolePart::Literals(parts);
        }
        // A `string`/`number` intrinsic, a free type parameter, an `infer` binder, or any
        // other symbolic type — not constructible.
        HolePart::NonLiteral
    }

    /// Assemble a mapped type's output properties (M26) after its key source has been
    /// evaluated (its result is on top of the value stack). Homomorphic: iterate the
    /// source properties ([`Self::homomorphic_source_props`] — a plain object's members,
    /// or the common-key intersection for the direct-union form), starting each output
    /// property's `?`/`readonly` from the source property (homomorphic preservation) and
    /// applying the node's modifier arithmetic, with `T[K]` resolved to the source
    /// property's type. Non-homomorphic: one property per string-literal key, starting
    /// both flags absent. Schedules a per-property value evaluation (the value template
    /// may itself demand conditional evaluation) and a final [`Task::BuildMappedObject`].
    ///
    /// **No permissive fallback (review F1 root cause):** a key source this subset
    /// cannot iterate — an index-signature source, a primitive, a non-literal key set —
    /// leaves the node **DEFERRED** (its own value, conservative relations), never an
    /// accept-everything `{}`. An error/any key source (an unresolved name upstream)
    /// degrades to the **error type** instead (M22 cascade suppression).
    fn assemble_mapped(
        &mut self,
        ty: TypeId,
        tasks: &mut Vec<Task>,
        values: &mut Vec<TypeId>,
        error: TypeId,
    ) {
        let key_source = values.pop().unwrap_or(error);
        let Some(mapped) = self.interner.store().mapped_type(ty).copied() else {
            // Defensive: leave the node as its own (deferred) value; SetMemo pops it.
            // (A hand-built node without a mapped row carries no modifiers source, so
            // there is no second stack value to pop here.)
            values.push(ty);
            return;
        };
        // M28: the (evaluated) modifiers source sits under the key source on the value
        // stack — pop it unconditionally whenever the node carries one, keeping the
        // stack arity balanced on every early return below.
        let modifiers_source = mapped
            .modifiers_source
            .map(|_| values.pop().unwrap_or(error));
        // M22 discipline: an error/any key source (e.g. `keyof Bogus` after TK2304)
        // degrades the whole result to the error type — cascades stay suppressed.
        let wk = self.interner.well_known();
        if key_source == wk.error || key_source == wk.any {
            values.push(error);
            return;
        }

        let mut meta: Vec<MappedProp> = Vec::new();
        let mut value_pre: Vec<TypeId> = Vec::new();
        if mapped.homomorphic {
            let Some(props) = self.homomorphic_source_props(key_source) else {
                // Non-iterable source (index signatures, primitives, …): out of the M26
                // subset — the node stays deferred, never a permissive `{}`.
                values.push(ty);
                return;
            };
            for prop in props {
                let value = self.replace_mapped_value(mapped.value_template, prop.ty);
                meta.push(MappedProp {
                    name: prop.name,
                    optional: mapped.optional_modifier.apply(prop.optional),
                    readonly: mapped.readonly_modifier.apply(prop.readonly),
                    // `-?` over an optional source member strips `undefined` from the
                    // evaluated value (tsc Required semantics; applied in
                    // `build_mapped_object` once the value has resolved).
                    strip_undefined: mapped.optional_modifier == ModifierOp::Remove
                        && prop.optional,
                });
                value_pre.push(value);
            }
        } else {
            // Non-homomorphic: the key set is the string-literal members of the key
            // source. A key set with any non-string-literal member (`K in string`, a
            // numeric key) is out of subset → deferred.
            let Some(names) = self.literal_string_keys(key_source) else {
                values.push(ty);
                return;
            };
            // M28: a captured **modifiers source** (`{ [P in K]: T[P] }` — tsc's
            // modifiersType) resolves each key against the source object: the
            // property's value type replaces the `MappedValue` placeholder and its
            // `?`/`readonly` flags seed the modifier arithmetic — so `Pick` preserves
            // both. Without one (Record's bare `V`), or for a key the source lacks
            // (`Pick<P, "q">` after its TK2344 — tsc still instantiates), the M26
            // behavior is unchanged: placeholder → error type, flags start absent.
            for name in names {
                let source_prop = modifiers_source
                    .and_then(|source| self.modifiers_source_property(source, &name));
                match source_prop {
                    Some(prop) => {
                        let value = self.replace_mapped_value(mapped.value_template, prop.ty);
                        meta.push(MappedProp {
                            name,
                            optional: mapped.optional_modifier.apply(prop.optional),
                            readonly: mapped.readonly_modifier.apply(prop.readonly),
                            // `-?` over an optional source member strips `undefined`,
                            // exactly like the homomorphic path.
                            strip_undefined: mapped.optional_modifier == ModifierOp::Remove
                                && prop.optional,
                        });
                        value_pre.push(value);
                    }
                    None => {
                        let value = self.replace_mapped_value(mapped.value_template, error);
                        meta.push(MappedProp {
                            name,
                            optional: mapped.optional_modifier.apply(false),
                            readonly: mapped.readonly_modifier.apply(false),
                            strip_undefined: false,
                        });
                        value_pre.push(value);
                    }
                }
            }
        }

        tasks.push(Task::BuildMappedObject(meta));
        // Push in reverse so the per-property values pop (and their results land) in
        // order, aligning with the metadata order in `BuildMappedObject`.
        for &v in value_pre.iter().rev() {
            tasks.push(Task::Eval(v));
        }
    }

    fn modifiers_source_property(&mut self, source: TypeId, name: &str) -> Option<PropertyType> {
        if let Some(prop) = self
            .interner
            .store()
            .object_type(source)
            .and_then(|object| Self::named_source_property(object, name))
        {
            return Some(prop);
        }

        if let Some(members) = self
            .interner
            .store()
            .intersection_members(source)
            .map(|m| m.to_vec())
        {
            return self.intersection_source_property(&members, name);
        }

        let members = self.interner.store().union_members(source)?.to_vec();
        let mut tys = Vec::with_capacity(members.len());
        let mut optional = false;
        let mut readonly = false;
        for member in members {
            let prop = self
                .interner
                .store()
                .object_type(member)
                .and_then(|object| Self::named_source_property(object, name))?;
            tys.push(prop.ty);
            optional |= prop.optional;
            readonly |= prop.readonly;
        }

        let mut prop = PropertyType::public(name.to_string(), self.interner.union(tys));
        prop.optional = optional;
        prop.readonly = readonly;
        Some(prop)
    }

    fn named_source_property(object: &ObjectType, name: &str) -> Option<PropertyType> {
        if let Some(prop) = object.property(name) {
            return Some(prop.clone());
        }
        object
            .string_index
            .map(|ty| PropertyType::public(name.to_string(), ty))
    }

    fn intersection_source_property(
        &mut self,
        members: &[TypeId],
        name: &str,
    ) -> Option<PropertyType> {
        let mut tys = Vec::with_capacity(members.len());
        let mut optional = true;
        let mut readonly = false;
        {
            let store = self.interner.store();
            for &member in members {
                let prop = store
                    .object_type(member)
                    .and_then(|object| Self::named_source_property(object, name))?;
                tys.push(prop.ty);
                optional &= prop.optional;
                readonly |= prop.readonly;
            }
        }

        let mut prop = PropertyType::public(name.to_string(), self.interner.intersection(tys));
        prop.optional = optional;
        prop.readonly = readonly;
        Some(prop)
    }

    /// The source properties a homomorphic map iterates (M26), or `None` when the key
    /// source is not iterable in this subset (the node then stays deferred):
    ///
    ///  - a plain **object** (no index signatures) → its properties (`{}` included:
    ///    `Ident<{}>` = `{}`);
    ///  - a **union of plain objects** — only reachable as the DIRECT
    ///    `{ [K in keyof (A | B)]: … }` form, since a substituted naked-param union
    ///    distributes in `substitute` before evaluation — → the **common-key**
    ///    intersection (tsc: `keyof (A | B)` = `keyof A & keyof B`), each common
    ///    property's type the union of the members' types, `?`/`readonly` OR-ed across
    ///    members (matching tsc's union-property synthesis);
    ///  - an **intersection of plain objects** → all member properties, duplicate keys
    ///    intersected;
    ///  - an object **with** index signatures (no `K in string` production), a
    ///    primitive, or any other shape → `None`.
    fn homomorphic_source_props(&mut self, key_source: TypeId) -> Option<Vec<PropertyType>> {
        if let Some(object) = self.interner.store().object_type(key_source) {
            if object.string_index.is_some() || object.number_index.is_some() {
                return None;
            }
            return Some(object.properties.clone());
        }
        if let Some(members) = self
            .interner
            .store()
            .intersection_members(key_source)
            .map(|m| m.to_vec())
        {
            return self.intersection_source_props(&members);
        }
        let members = self.interner.store().union_members(key_source)?.to_vec();
        let mut member_objects: Vec<Vec<PropertyType>> = Vec::with_capacity(members.len());
        {
            let store = self.interner.store();
            for member in &members {
                let object = store.object_type(*member)?;
                if object.string_index.is_some() || object.number_index.is_some() {
                    return None;
                }
                member_objects.push(object.properties.clone());
            }
        }
        // Intersect: keep the first member's keys present in EVERY member, collecting
        // each member's value type + flags. (A union always has ≥ 2 members.)
        let (first, rest) = member_objects.split_first()?;
        let mut common: Vec<(String, Vec<TypeId>, bool, bool)> = Vec::new();
        for prop in first {
            let mut tys = vec![prop.ty];
            let mut optional = prop.optional;
            let mut readonly = prop.readonly;
            let mut in_all = true;
            for other in rest {
                match other.iter().find(|p| p.name == prop.name) {
                    Some(p) => {
                        tys.push(p.ty);
                        optional |= p.optional;
                        readonly |= p.readonly;
                    }
                    None => {
                        in_all = false;
                        break;
                    }
                }
            }
            if in_all {
                common.push((prop.name.clone(), tys, optional, readonly));
            }
        }
        // Union the per-member value types outside the store borrow.
        let mut out: Vec<PropertyType> = Vec::with_capacity(common.len());
        for (name, tys, optional, readonly) in common {
            let ty = self.interner.union(tys);
            let mut prop = PropertyType::public(name, ty);
            prop.optional = optional;
            prop.readonly = readonly;
            out.push(prop);
        }
        Some(out)
    }

    fn intersection_source_props(&mut self, members: &[TypeId]) -> Option<Vec<PropertyType>> {
        let mut entries: Vec<(String, Vec<TypeId>, bool, bool)> = Vec::new();
        {
            let store = self.interner.store();
            for &member in members {
                let object = store.object_type(member)?;
                if object.string_index.is_some() || object.number_index.is_some() {
                    return None;
                }
                for prop in &object.properties {
                    match entries
                        .iter_mut()
                        .find(|(name, _, _, _)| *name == prop.name)
                    {
                        Some((_, tys, optional, readonly)) => {
                            tys.push(prop.ty);
                            *optional &= prop.optional;
                            *readonly |= prop.readonly;
                        }
                        None => entries.push((
                            prop.name.clone(),
                            vec![prop.ty],
                            prop.optional,
                            prop.readonly,
                        )),
                    }
                }
            }
        }

        let mut props = Vec::with_capacity(entries.len());
        for (name, tys, optional, readonly) in entries {
            let mut prop = PropertyType::public(name, self.interner.intersection(tys));
            prop.optional = optional;
            prop.readonly = readonly;
            props.push(prop);
        }
        Some(props)
    }

    /// Build the mapped result object, preserving metadata by position. `-?` over
    /// optional strips `undefined`; optional outputs bake `| undefined` into the
    /// stored type for the relation engine.
    fn build_mapped_object(&mut self, meta: &[MappedProp], values: &[TypeId]) -> TypeId {
        let undefined = self.interner.well_known().undefined;
        let mut object = ObjectType::default();
        for (m, &value) in meta.iter().zip(values) {
            // `-?` Required semantics: strip `undefined` from the EVALUATED value of an
            // optional source member (probed tsc 6.0.3 — see `strip_undefined`).
            let value = if m.strip_undefined {
                self.strip_undefined(value)
            } else {
                value
            };
            // M21: an optional member's stored effective type includes `| undefined`, so
            // the relation engine's optional handling stays consistent.
            let ty = if m.optional {
                self.interner.union(vec![value, undefined])
            } else {
                value
            };
            let mut prop = PropertyType::public(m.name.clone(), ty);
            prop.optional = m.optional;
            prop.readonly = m.readonly;
            object.properties.push(prop);
        }
        self.interner.intern_object(object)
    }

    /// Remove `undefined` from a value type (M26 — the `-?` Required strip, probed
    /// against tsc 6.0.3, leader-arbitrated `m26_arb.ts`): a union containing
    /// `undefined` re-unions its other members (a 1-member remainder collapses via
    /// `Interner::union`); a value that is EXACTLY `undefined` maps to **`never`**
    /// (`Required<{ b?: undefined }>` gives `b: never` — filtering `undefined` by the
    /// not-undefined fact leaves nothing); any other non-union type is untouched.
    fn strip_undefined(&mut self, ty: TypeId) -> TypeId {
        let wk = self.interner.well_known();
        if ty == wk.undefined {
            return wk.never;
        }
        let filtered: Vec<TypeId> = match self.interner.store().union_members(ty) {
            Some(members) if members.contains(&wk.undefined) => members
                .iter()
                .copied()
                .filter(|&m| m != wk.undefined)
                .collect(),
            _ => return ty,
        };
        self.interner.union(filtered)
    }

    /// The string-literal keys of a non-homomorphic mapped type's key source (M26): the
    /// members of a literal union, or a single string literal. `None` — deferring the
    /// node — when ANY member is not a string literal (`K in string`, numeric keys, …):
    /// silently dropping such a key would shrink the target (a missed-member false
    /// negative), so the whole map is out of subset instead (review F1 secondaries).
    fn literal_string_keys(&self, ty: TypeId) -> Option<Vec<String>> {
        let store = self.interner.store();
        let members: Vec<TypeId> = match store.union_members(ty) {
            Some(members) => members.to_vec(),
            None => vec![ty],
        };
        members
            .into_iter()
            .map(|m| match store.literal_value(m) {
                Some(LiteralValue::String(s)) => Some(s.clone()),
                _ => None,
            })
            .collect()
    }

    /// Replace every [`TypeTag::MappedValue`] placeholder (`T[K]`) in a mapped type's
    /// value template with `value` — the current key's source property type (M26).
    /// Recurses through structural composites and a conditional's components; re-interns
    /// only when something changed. A **nested** mapped type descends into its
    /// **key source only** (which is outer-scoped — `Outer<T> = { [K in keyof T]:
    /// Ident<T[K]> }` injects the outer placeholder there, review probe X): its value
    /// template rebinds its OWN placeholder and stays untouched (the cross-binder case
    /// is out of subset, safe over-report).
    fn replace_mapped_value(&mut self, ty: TypeId, value: TypeId) -> TypeId {
        match self.interner.store().tag(ty) {
            TypeTag::MappedValue => value,
            TypeTag::Intrinsic | TypeTag::Literal | TypeTag::TypeParam | TypeTag::Infer => ty,
            TypeTag::Mapped => {
                let Some(mapped) = self.interner.store().mapped_type(ty).copied() else {
                    return ty;
                };
                let key_source = self.replace_mapped_value(mapped.key_source, value);
                // M28: the modifiers source is outer-scoped like the key source (the
                // captured `T` of a nested `{ [P in K]: T[P] }` may be the OUTER map's
                // placeholder) — descend into it too.
                let modifiers_source = mapped
                    .modifiers_source
                    .map(|ms| self.replace_mapped_value(ms, value));
                if key_source == mapped.key_source && modifiers_source == mapped.modifiers_source {
                    return ty;
                }
                self.interner.intern_mapped(MappedType {
                    key_source,
                    modifiers_source,
                    ..mapped
                })
            }
            TypeTag::Object => {
                let Some(object) = self.interner.store().object_type(ty).cloned() else {
                    return ty;
                };
                let mut changed = false;
                let mut new = object.clone();
                for prop in &mut new.properties {
                    let nt = self.replace_mapped_value(prop.ty, value);
                    changed |= nt != prop.ty;
                    prop.ty = nt;
                }
                new.string_index = object.string_index.map(|v| {
                    let nv = self.replace_mapped_value(v, value);
                    changed |= nv != v;
                    nv
                });
                new.number_index = object.number_index.map(|v| {
                    let nv = self.replace_mapped_value(v, value);
                    changed |= nv != v;
                    nv
                });
                new.call_signatures = object
                    .call_signatures
                    .iter()
                    .map(|&s| {
                        let ns = self.replace_mapped_value(s, value);
                        changed |= ns != s;
                        ns
                    })
                    .collect();
                new.construct_signatures = object
                    .construct_signatures
                    .iter()
                    .map(|&s| {
                        let ns = self.replace_mapped_value(s, value);
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
                    let nt = self.replace_mapped_value(param.ty, value);
                    changed |= nt != param.ty;
                    param.ty = nt;
                }
                let nr = self.replace_mapped_value(function.ret, value);
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
                        let nm = self.replace_mapped_value(m, value);
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
            // M31: descend into intersection members like a union, re-interning through
            // `Interner::intersection` only when a member changed.
            TypeTag::Intersection => {
                let Some(members) = self.interner.store().intersection_members(ty) else {
                    return ty;
                };
                let members: Vec<TypeId> = members.to_vec();
                let mut changed = false;
                let subst: Vec<TypeId> = members
                    .iter()
                    .map(|&m| {
                        let nm = self.replace_mapped_value(m, value);
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
                let ne = self.replace_mapped_value(element, value);
                if ne != element {
                    self.interner.intern_array(ne)
                } else {
                    ty
                }
            }
            TypeTag::Tuple => {
                let Some(elements) = self
                    .interner
                    .store()
                    .tuple_type(ty)
                    .map(|t| t.elements.clone())
                else {
                    return ty;
                };
                let mut changed = false;
                let subst: Vec<TypeId> = elements
                    .iter()
                    .map(|&e| {
                        let ne = self.replace_mapped_value(e, value);
                        changed |= ne != e;
                        ne
                    })
                    .collect();
                if changed {
                    self.interner.intern_tuple(subst)
                } else {
                    ty
                }
            }
            TypeTag::Readonly => {
                let Some(operand) = self.interner.store().readonly_operand(ty) else {
                    return ty;
                };
                let no = self.replace_mapped_value(operand, value);
                if no != operand {
                    self.interner.intern_readonly(no)
                } else {
                    ty
                }
            }
            TypeTag::Conditional => {
                let Some(cond) = self.interner.store().conditional_type(ty).copied() else {
                    return ty;
                };
                let check = self.replace_mapped_value(cond.check, value);
                let extends_ty = self.replace_mapped_value(cond.extends_ty, value);
                let true_branch = self.replace_mapped_value(cond.true_branch, value);
                let false_branch = self.replace_mapped_value(cond.false_branch, value);
                if check == cond.check
                    && extends_ty == cond.extends_ty
                    && true_branch == cond.true_branch
                    && false_branch == cond.false_branch
                {
                    return ty;
                }
                self.interner.intern_conditional(ConditionalType {
                    check,
                    extends_ty,
                    true_branch,
                    false_branch,
                    infer_count: cond.infer_count,
                    distributive: cond.distributive,
                    poisoned: cond.poisoned,
                })
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
                        let nv = self.replace_mapped_value(v, value);
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
            // M27: a template's `T[K]` placeholder lives in its holes
            // (`` `x${T[K]}` `` inside a mapped value template) — recurse into them.
            TypeTag::Template => {
                let Some(template) = self.interner.store().template_type(ty).cloned() else {
                    return ty;
                };
                let mut changed = false;
                let new_holes: Vec<TypeId> = template
                    .holes
                    .iter()
                    .map(|&hole| {
                        let nh = self.replace_mapped_value(hole, value);
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
            // M28: a `keyof X[K]`-style value template carries the placeholder in the
            // keyof operand — recurse into it.
            TypeTag::Keyof => {
                let Some(operand) = self.interner.store().keyof_operand(ty) else {
                    return ty;
                };
                let no = self.replace_mapped_value(operand, value);
                if no != operand {
                    self.interner.intern_keyof(no)
                } else {
                    ty
                }
            }
        }
    }

    /// Run the `extends` test for a concrete conditional, returning
    /// `(matched, true_branch_with_infers_substituted)`. With `infer` binders present,
    /// their node-scoped de Bruijn indices are freshened to transient type parameters,
    /// candidates are collected through [`infer_from_types_for_conditional`] (same-name occurrences union
    /// — the covariant rule), and the matched candidates are substituted into both the
    /// `extends` type (before the relation test) and the true branch.
    fn run_extends_test(&mut self, cond: &ConditionalType) -> (bool, TypeId) {
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

    /// Replace each `infer` binder (de Bruijn index `i`) with `fresh[i]`, recursing
    /// through structural composites and instantiation argument values, but **not** into
    /// a nested conditional (which rebinds its own indices — M25 does not model nested
    /// `infer`). Re-interns only when something changed.
    fn substitute_infers(&mut self, ty: TypeId, fresh: &[TypeId]) -> TypeId {
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
                let Some(elements) = self
                    .interner
                    .store()
                    .tuple_type(ty)
                    .map(|t| t.elements.clone())
                else {
                    return ty;
                };
                let mut changed = false;
                let subst: Vec<TypeId> = elements
                    .iter()
                    .map(|&e| {
                        let ne = self.substitute_infers(e, fresh);
                        changed |= ne != e;
                        ne
                    })
                    .collect();
                if changed {
                    self.interner.intern_tuple(subst)
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

    /// `src <: tgt` through the existing relation engine (a fresh [`Relater`] per test —
    /// the cache/cycle-stack invariants are untouched; the store is borrowed read-only
    /// while the relater lives, then released before the next interning step).
    fn is_assignable(&self, src: TypeId, tgt: TypeId) -> bool {
        let wk = self.interner.well_known();
        let store = self.interner.store();
        let mut relater = Relater::new(store, wk);
        relater.is_assignable(src, tgt).is_yes()
    }

    /// Whether `ty` contains **no** free declaration type parameter (so a conditional
    /// whose check is `ty` may be evaluated). Iterative with a cache + cycle guard, so a
    /// 10 000-deep check type is classified without native-stack recursion and repeated
    /// sub-terms are `O(1)`.
    fn is_concrete(&mut self, ty: TypeId) -> bool {
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
    fn child_types(&self, ty: TypeId) -> Vec<TypeId> {
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
                .map(|t| t.elements.clone())
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

/// Compute `keyof` over a concrete object or union of objects. Object operands use
/// [`keyof_of_object`]; union operands intersect the members' known keys (`keyof (A | B)`
/// is the keys common to every member). `None` means the operand shape is outside this
/// subset and the caller decides the fallback.
pub(in crate::check::checker) fn keyof_of_type(
    interner: &mut Interner,
    operand: TypeId,
) -> Option<TypeId> {
    if interner.store().object_type(operand).is_some() {
        return keyof_of_object(interner, operand);
    }
    if let Some(members) = interner
        .store()
        .intersection_members(operand)
        .map(|m| m.to_vec())
    {
        let mut keys = Vec::with_capacity(members.len());
        for member in members {
            keys.push(keyof_of_type(interner, member)?);
        }
        return Some(interner.union(keys));
    }
    let members = interner.store().union_members(operand)?.to_vec();
    keyof_of_union(interner, &members)
}

/// Compute `keyof` over an **object** operand — shared by eager lowering and deferred
/// evaluation. The result is the `union(...)` of property names as string-literal
/// types, plus `string`/`number` for the respective index signatures (an empty object
/// yields `never` via the union collapse). `None` when the operand is not an object.
pub(in crate::check::checker) fn keyof_of_object(
    interner: &mut Interner,
    operand: TypeId,
) -> Option<TypeId> {
    let store = interner.store();
    let object = store.object_type(operand)?;

    // Snapshot the key components before the mutable interning borrow.
    let names: Vec<String> = object.properties.iter().map(|p| p.name.clone()).collect();
    let has_string_index = object.string_index.is_some();
    let has_number_index = object.number_index.is_some();

    let wk = interner.well_known();
    let mut members: Vec<TypeId> = Vec::with_capacity(names.len() + 2);
    for name in names {
        members.push(interner.intern_literal(LiteralValue::String(name)));
    }
    if has_string_index {
        members.push(wk.string);
    }
    if has_number_index {
        members.push(wk.number);
    }
    Some(interner.union(members))
}

struct UnionKeyInfo {
    names: Vec<String>,
    name_set: FxHashSet<String>,
    has_string_index: bool,
    has_number_index: bool,
}

fn keyof_of_union(interner: &mut Interner, members: &[TypeId]) -> Option<TypeId> {
    let infos: Vec<UnionKeyInfo> = {
        let store = interner.store();
        let mut infos = Vec::with_capacity(members.len());
        for &member in members {
            let object = store.object_type(member)?;
            let names: Vec<String> = object.properties.iter().map(|p| p.name.clone()).collect();
            let name_set = names.iter().cloned().collect();
            infos.push(UnionKeyInfo {
                names,
                name_set,
                has_string_index: object.string_index.is_some(),
                has_number_index: object.number_index.is_some(),
            });
        }
        infos
    };

    let wk = interner.well_known();
    let mut seen = FxHashSet::default();
    let mut all_names = Vec::new();
    for info in &infos {
        for name in &info.names {
            if seen.insert(name.clone()) {
                all_names.push(name.clone());
            }
        }
    }

    let mut keys = Vec::new();
    for name in all_names {
        if infos
            .iter()
            .all(|info| info.name_set.contains(&name) || info.has_string_index)
        {
            keys.push(interner.intern_literal(LiteralValue::String(name)));
        }
    }
    if infos.iter().all(|info| info.has_string_index) {
        keys.push(wk.string);
    }
    if infos.iter().all(|info| info.has_number_index) {
        keys.push(wk.number);
    }
    Some(interner.union(keys))
}

/// Whether `ty` transitively contains a genuinely deferred [`TypeTag::Keyof`] node
/// (M28) — the constraint-side gate for a `keyof` whose operand still contains a free
/// declaration type parameter. Concrete-but-unsupported `keyof` nodes must not be
/// skipped; they relate conservatively instead of accepting bad arguments.
pub(in crate::check) fn contains_deferred_keyof(
    store: &crate::types::store::Store,
    ty: TypeId,
) -> bool {
    contains_deferred_keyof_node(store, ty)
}

fn contains_deferred_keyof_node(store: &crate::types::store::Store, ty: TypeId) -> bool {
    let mut stack = vec![ty];
    let mut visited: FxHashSet<TypeId> = FxHashSet::default();
    while let Some(t) = stack.pop() {
        if !visited.insert(t) {
            continue;
        }
        if store.tag(t) == TypeTag::Keyof {
            if let Some(operand) = store.keyof_operand(t) {
                if contains_free_keyof_operand(store, operand) {
                    return true;
                }
                stack.push(operand);
            }
            continue;
        }
        push_node_children(store, t, &mut stack, false);
    }
    false
}

fn contains_free_keyof_operand(store: &crate::types::store::Store, ty: TypeId) -> bool {
    let mut stack = vec![ty];
    let mut visited: FxHashSet<TypeId> = FxHashSet::default();
    while let Some(t) = stack.pop() {
        if !visited.insert(t) {
            continue;
        }
        match store.tag(t) {
            TypeTag::TypeParam | TypeTag::Infer | TypeTag::MappedValue => return true,
            _ => push_node_children(store, t, &mut stack, false),
        }
    }
    false
}

fn push_node_children(
    store: &crate::types::store::Store,
    ty: TypeId,
    stack: &mut Vec<TypeId>,
    include_instantiation_base: bool,
) {
    match store.tag(ty) {
        TypeTag::Keyof => {
            if let Some(operand) = store.keyof_operand(ty) {
                stack.push(operand);
            }
        }
        TypeTag::Object => {
            if let Some(object) = store.object_type(ty) {
                stack.extend(object.properties.iter().map(|p| p.ty));
                stack.extend(object.string_index);
                stack.extend(object.number_index);
                stack.extend(object.call_signatures.iter().copied());
                stack.extend(object.construct_signatures.iter().copied());
            }
        }
        TypeTag::Function => {
            if let Some(f) = store.function_type(ty) {
                stack.extend(f.params.iter().map(|p| p.ty));
                stack.push(f.ret);
            }
        }
        TypeTag::Union => {
            if let Some(members) = store.union_members(ty) {
                stack.extend(members.iter().copied());
            }
        }
        TypeTag::Intersection => {
            if let Some(members) = store.intersection_members(ty) {
                stack.extend(members.iter().copied());
            }
        }
        TypeTag::Array => {
            if let Some(a) = store.array_type(ty) {
                stack.push(a.element);
            }
        }
        TypeTag::Tuple => {
            if let Some(tup) = store.tuple_type(ty) {
                stack.extend(tup.elements.iter().copied());
            }
        }
        TypeTag::Readonly => {
            if let Some(operand) = store.readonly_operand(ty) {
                stack.push(operand);
            }
        }
        TypeTag::Conditional => {
            if let Some(c) = store.conditional_type(ty) {
                stack.extend([c.check, c.extends_ty, c.true_branch, c.false_branch]);
            }
        }
        TypeTag::Instantiation => {
            if let Some(inst) = store.instantiation_type(ty) {
                if include_instantiation_base {
                    stack.push(inst.base);
                }
                stack.extend(inst.args.iter().map(|(_, v)| *v));
            }
        }
        TypeTag::Mapped => {
            if let Some(m) = store.mapped_type(ty) {
                stack.push(m.key_source);
                stack.push(m.value_template);
                stack.extend(m.modifiers_source);
            }
        }
        TypeTag::Template => {
            if let Some(template) = store.template_type(ty) {
                stack.extend(template.holes.iter().copied());
            }
        }
        TypeTag::Intrinsic
        | TypeTag::Literal
        | TypeTag::TypeParam
        | TypeTag::Infer
        | TypeTag::MappedValue => {}
    }
}

/// Whether `ty` transitively contains a **deferred type-level node** — a deferred
/// keyof, conditional, or lazy instantiation. Since M28 round 4 this is a
/// **message-form chooser only** (the round-3 TK2344 argument-side GATE it powered
/// was probe-disproven and removed — arguments now always evaluate and always
/// check): a failing argument whose evaluated form still trips this walk renders by
/// its WRITTEN form (which carries the alias name — `Extract<K, string>`) instead of
/// the raw substituted body.
pub(in crate::check) fn contains_deferred_argument(
    store: &crate::types::store::Store,
    ty: TypeId,
) -> bool {
    contains_nodes(store, ty, |tag| {
        matches!(
            tag,
            TypeTag::Keyof | TypeTag::Conditional | TypeTag::Instantiation
        )
    })
}

/// The shared deep walk behind the M28 undecidability gates: whether any node of a
/// target tag kind occurs anywhere inside `ty` (descending through every composite,
/// including conditional components, instantiation bases + argument values, mapped
/// components, and template holes). Iterative with a visited set so recursive
/// interned types terminate.
fn contains_nodes(
    store: &crate::types::store::Store,
    ty: TypeId,
    is_target: impl Fn(TypeTag) -> bool,
) -> bool {
    let mut stack = vec![ty];
    let mut visited: FxHashSet<TypeId> = FxHashSet::default();
    while let Some(t) = stack.pop() {
        if !visited.insert(t) {
            continue;
        }
        if is_target(store.tag(t)) {
            return true;
        }
        push_node_children(store, t, &mut stack, true);
    }
    false
}

/// Apply an M28 string intrinsic to a string literal. Upper/lowercase map the
/// whole string; capitalize/uncapitalize map only the first char.
fn transform_string_intrinsic(wk: &crate::types::WellKnown, base: TypeId, s: &str) -> String {
    if base == wk.uppercase {
        return s.to_uppercase();
    }
    if base == wk.lowercase {
        return s.to_lowercase();
    }
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => {
            let mapped: String = if base == wk.capitalize {
                first.to_uppercase().collect()
            } else {
                first.to_lowercase().collect()
            };
            mapped + chars.as_str()
        }
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::repr::{ObjectType, PropertyType};

    fn prop(name: &str, ty: TypeId) -> PropertyType {
        PropertyType::public(name, ty)
    }

    /// Witness (architecture §7.2 item b): a ~10 000-deep nested `{ v: … }` type
    /// evaluated by an `Unwrap`-style recursive conditional resolves to the innermost
    /// type **without overflowing the native stack**. Built programmatically via the
    /// interner (a parsed fixture would stress the parser instead), and run with a raised
    /// budget so the work-stack — not the step budget — is what proves termination.
    #[test]
    fn deep_recursive_unwrap_does_not_overflow_the_native_stack() {
        const DEPTH: usize = 10_000;
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();

        // The recursive template: `type Unwrap<T> = T extends { v: infer U } ? Unwrap<U> : T`.
        // T = TypeParamId(0); the true branch is a lazy self-instantiation carrying the
        // infer binder as the argument.
        let t = interner.intern_type_param(TypeParamId(0), "T");
        let infer0 = interner.intern_infer(0);
        let extends = interner.intern_object(ObjectType {
            properties: vec![prop("v", infer0)],
            ..Default::default()
        });
        let template = interner.reserve_conditional();
        let recur = interner.intern_instantiation(template, vec![(TypeParamId(0), infer0)]);
        interner.fill_conditional(
            template,
            ConditionalType {
                check: t,
                extends_ty: extends,
                true_branch: recur,
                false_branch: t,
                infer_count: 1,
                distributive: true,
                poisoned: false,
            },
        );

        // Build the 10 000-deep check type `{ v: { v: … { v: number } … } }`, innermost
        // out (iteratively — no recursion here either).
        let mut deep = wk.number;
        for _ in 0..DEPTH {
            deep = interner.intern_object(ObjectType {
                properties: vec![prop("v", deep)],
                ..Default::default()
            });
        }

        // `Unwrap<deep>` — evaluate with a budget above the depth so termination is the
        // work-stack's doing, not the budget's.
        let root = interner.intern_instantiation(template, vec![(TypeParamId(0), deep)]);
        let mut next_type_param: u32 = 1;
        let mut memo = FxHashMap::default();
        let mut ev = ConditionalEvaluator::new(
            &mut interner,
            &mut next_type_param,
            &mut memo,
            (DEPTH as u32) + 1000,
        );
        let result = ev.evaluate(root);
        assert!(!ev.exhausted, "the raised budget must not be exhausted");
        assert_eq!(
            result, wk.number,
            "Unwrap fully descends to the innermost `number`"
        );
    }

    /// A terminating shallow `Unwrap` resolves, and its memo is populated (a repeat
    /// evaluation is a cache hit).
    #[test]
    fn shallow_unwrap_resolves_and_memoizes() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let t = interner.intern_type_param(TypeParamId(0), "T");
        let infer0 = interner.intern_infer(0);
        let extends = interner.intern_object(ObjectType {
            properties: vec![prop("v", infer0)],
            ..Default::default()
        });
        let template = interner.reserve_conditional();
        let recur = interner.intern_instantiation(template, vec![(TypeParamId(0), infer0)]);
        interner.fill_conditional(
            template,
            ConditionalType {
                check: t,
                extends_ty: extends,
                true_branch: recur,
                false_branch: t,
                infer_count: 1,
                distributive: true,
                poisoned: false,
            },
        );
        // `{ v: { v: number } }`.
        let inner = interner.intern_object(ObjectType {
            properties: vec![prop("v", wk.number)],
            ..Default::default()
        });
        let outer = interner.intern_object(ObjectType {
            properties: vec![prop("v", inner)],
            ..Default::default()
        });
        let root = interner.intern_instantiation(template, vec![(TypeParamId(0), outer)]);

        let mut next_type_param: u32 = 1;
        let mut memo = FxHashMap::default();
        let mut ev = ConditionalEvaluator::new(
            &mut interner,
            &mut next_type_param,
            &mut memo,
            DEFAULT_STEP_BUDGET,
        );
        assert_eq!(ev.evaluate(root), wk.number);
        assert!(memo.contains_key(&root), "the root evaluation is memoized");
    }

    /// A **poisoned** conditional (cross-binder nested infer — backlog 26 stopgap) NEVER
    /// evaluates, even with a fully concrete check: the evaluator returns the node
    /// as-is (both directly and through an instantiation of a poisoned template), so it
    /// stays a deferred node under the conservative relation rules.
    #[test]
    fn poisoned_conditional_never_evaluates() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let s_lit = interner.intern_literal(LiteralValue::String("s".into()));
        let n_lit = interner.intern_literal(LiteralValue::String("n".into()));

        // A concrete-check poisoned node: `string extends string ? "s" : "n"` — would
        // resolve to "s" if evaluation were allowed.
        let poisoned = interner.intern_conditional(ConditionalType {
            check: wk.string,
            extends_ty: wk.string,
            true_branch: s_lit,
            false_branch: n_lit,
            infer_count: 0,
            distributive: false,
            poisoned: true,
        });
        let mut next_type_param: u32 = 0;
        let mut memo = FxHashMap::default();
        let mut ev = ConditionalEvaluator::new(
            &mut interner,
            &mut next_type_param,
            &mut memo,
            DEFAULT_STEP_BUDGET,
        );
        assert_eq!(
            ev.evaluate(poisoned),
            poisoned,
            "a poisoned conditional must be returned as-is, never evaluated"
        );
        assert!(!ev.exhausted);
        drop(ev);

        // Through an instantiation of a poisoned distributive template: the expansion
        // must NOT distribute (a poisoned base is treated as non-distributive) — the
        // result is the substituted, still-poisoned node, unevaluated.
        let t = interner.intern_type_param(TypeParamId(900), "T");
        let template = interner.intern_conditional(ConditionalType {
            check: t,
            extends_ty: wk.string,
            true_branch: s_lit,
            false_branch: n_lit,
            infer_count: 0,
            distributive: true,
            poisoned: true,
        });
        let union = interner.union(vec![wk.string, wk.number]);
        let root = interner.intern_instantiation(template, vec![(TypeParamId(900), union)]);
        let mut ev = ConditionalEvaluator::new(
            &mut interner,
            &mut next_type_param,
            &mut memo,
            DEFAULT_STEP_BUDGET,
        );
        let result = ev.evaluate(root);
        drop(ev);
        let out = interner
            .store()
            .conditional_type(result)
            .copied()
            .expect("the instantiation must resolve to a (deferred) conditional node");
        assert!(out.poisoned, "the substituted node stays poisoned");
        assert_eq!(out.check, union, "substituted once, never distributed");
    }

    /// A genuinely-infinite alias (`type Inf<T> = T extends {} ? Inf<{ v: T }> : never`)
    /// trips the step budget rather than looping, setting `exhausted`.
    #[test]
    fn runaway_growth_exhausts_the_budget() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let t = interner.intern_type_param(TypeParamId(0), "T");
        let empty = interner.intern_object(ObjectType::default());
        let template = interner.reserve_conditional();
        // The true branch wraps the check type: `Inf<{ v: T }>`.
        let wrapped = interner.intern_object(ObjectType {
            properties: vec![prop("v", t)],
            ..Default::default()
        });
        let recur = interner.intern_instantiation(template, vec![(TypeParamId(0), wrapped)]);
        interner.fill_conditional(
            template,
            ConditionalType {
                check: t,
                extends_ty: empty,
                true_branch: recur,
                false_branch: wk.never,
                infer_count: 0,
                distributive: true,
                poisoned: false,
            },
        );
        let root = interner.intern_instantiation(template, vec![(TypeParamId(0), empty)]);

        let mut next_type_param: u32 = 1;
        let mut memo = FxHashMap::default();
        let mut ev = ConditionalEvaluator::new(
            &mut interner,
            &mut next_type_param,
            &mut memo,
            DEFAULT_STEP_BUDGET,
        );
        let _ = ev.evaluate(root);
        assert!(ev.exhausted, "a runaway alias must exhaust the step budget");
    }

    /// M26 — a homomorphic identity mapped type `{ [K in keyof T]: T[K] }` over a
    /// concrete source evaluates to the source's shape (per-property `T[K]` = the source
    /// property's type), and its result is memoized.
    fn eval(
        interner: &mut Interner,
        next: &mut u32,
        memo: &mut FxHashMap<TypeId, TypeId>,
        ty: TypeId,
    ) -> TypeId {
        let mut ev = ConditionalEvaluator::new(interner, next, memo, DEFAULT_STEP_BUDGET);
        ev.evaluate(ty)
    }

    #[test]
    fn mapped_identity_evaluates_to_source_shape() {
        use crate::types::repr::{MappedType, ModifierOp};
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        // Concrete source `{ a: number; b: string }`.
        let source = interner.intern_object(ObjectType {
            properties: vec![prop("a", wk.number), prop("b", wk.string)],
            ..Default::default()
        });
        let placeholder = interner.intern_mapped_value();
        let ident = interner.intern_mapped(MappedType {
            homomorphic: true,
            key_source: source,
            value_template: placeholder,
            modifiers_source: None,
            optional_modifier: ModifierOp::Keep,
            readonly_modifier: ModifierOp::Keep,
        });
        let mut next = 0u32;
        let mut memo = FxHashMap::default();
        let result = eval(&mut interner, &mut next, &mut memo, ident);
        assert_eq!(
            result, source,
            "an identity map over a concrete source yields the source shape"
        );
        assert!(
            memo.contains_key(&ident),
            "the mapped evaluation is memoized"
        );
    }

    /// M26 — modifier arithmetic: `readonly` (Add) sets every result property readonly;
    /// `?` (Add) makes every property optional; a `MappedValue | null` template unions
    /// `null` into each value.
    #[test]
    fn mapped_modifiers_and_value_transform_apply() {
        use crate::types::repr::{MappedType, ModifierOp};
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let source = interner.intern_object(ObjectType {
            properties: vec![prop("a", wk.number)],
            ..Default::default()
        });
        let placeholder = interner.intern_mapped_value();
        // `{ readonly [K in keyof T]?: T[K] | null }`.
        let value_template = interner.union(vec![placeholder, wk.null]);
        let mapped = interner.intern_mapped(MappedType {
            homomorphic: true,
            key_source: source,
            value_template,
            modifiers_source: None,
            optional_modifier: ModifierOp::Add,
            readonly_modifier: ModifierOp::Add,
        });
        let mut next = 0u32;
        let mut memo = FxHashMap::default();
        let result = eval(&mut interner, &mut next, &mut memo, mapped);

        let a = interner
            .store()
            .object_type(result)
            .and_then(|o| o.property("a"))
            .expect("property a survives")
            .clone();
        assert!(a.readonly, "readonly (Add) makes the property readonly");
        assert!(a.optional, "? (Add) makes the property optional");
        // Effective type is `number | null | undefined` (value `number | null`, plus the
        // optional `| undefined` baked in).
        let expected = interner.union(vec![wk.number, wk.null, wk.undefined]);
        assert_eq!(
            a.ty, expected,
            "value template `T[K] | null` + optional `| undefined`"
        );
    }

    /// M27 — template construction: all-literal holes **collapse** (`` `a-${"b"}` `` →
    /// `"a-b"`), a union hole distributes to the cartesian-product union, `boolean`
    /// expands to `"false" | "true"`, a `never` hole short-circuits to `never`, and a
    /// number literal stringifies.
    #[test]
    fn template_construction_collapses_and_distributes() {
        use crate::types::repr::TemplateType;
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let mut next = 0u32;
        let mut memo = FxHashMap::default();

        let s = |interner: &mut Interner, v: &str| {
            interner.intern_literal(LiteralValue::String(v.to_string()))
        };
        let template = |interner: &mut Interner, texts: &[&str], holes: Vec<TypeId>| {
            interner.intern_template(TemplateType {
                texts: texts.iter().map(|t| t.to_string()).collect(),
                holes,
            })
        };

        // `` `a-${"b"}` `` → "a-b".
        let b = s(&mut interner, "b");
        let one = template(&mut interner, &["a-", ""], vec![b]);
        let expect = s(&mut interner, "a-b");
        assert_eq!(eval(&mut interner, &mut next, &mut memo, one), expect);

        // `` `${"a"|"b"}-${"1"|"2"}` `` → "a-1" | "a-2" | "b-1" | "b-2".
        let a = s(&mut interner, "a");
        let b = s(&mut interner, "b");
        let d1 = s(&mut interner, "1");
        let d2 = s(&mut interner, "2");
        let ab = interner.union(vec![a, b]);
        let d12 = interner.union(vec![d1, d2]);
        let two = template(&mut interner, &["", "-", ""], vec![ab, d12]);
        let members: Vec<TypeId> = ["a-1", "a-2", "b-1", "b-2"]
            .into_iter()
            .map(|v| s(&mut interner, v))
            .collect();
        let expect = interner.union(members);
        assert_eq!(eval(&mut interner, &mut next, &mut memo, two), expect);

        // `` `is:${boolean}` `` → "is:false" | "is:true".
        let bh = template(&mut interner, &["is:", ""], vec![wk.boolean]);
        let f = s(&mut interner, "is:false");
        let t = s(&mut interner, "is:true");
        let expect = interner.union(vec![f, t]);
        assert_eq!(eval(&mut interner, &mut next, &mut memo, bh), expect);

        // `` `x${never}` `` → never.
        let nh = template(&mut interner, &["x", ""], vec![wk.never]);
        assert_eq!(eval(&mut interner, &mut next, &mut memo, nh), wk.never);

        // `` `v${1|2}` `` → "v1" | "v2" (number stringify).
        let n1 = interner.intern_literal(LiteralValue::Number(1.0));
        let n2 = interner.intern_literal(LiteralValue::Number(2.0));
        let n12 = interner.union(vec![n1, n2]);
        let ver = template(&mut interner, &["v", ""], vec![n12]);
        let v1 = s(&mut interner, "v1");
        let v2 = s(&mut interner, "v2");
        let expect = interner.union(vec![v1, v2]);
        assert_eq!(eval(&mut interner, &mut next, &mut memo, ver), expect);
    }

    /// M27 — a template with a **non-literal** hole (a `string` intrinsic, or a free
    /// declaration type parameter) stays a **symbolic** node; an **error-typed** hole
    /// degrades the whole template to the error type (M22 cascade suppression).
    #[test]
    fn template_construction_keeps_symbolic_and_suppresses_error() {
        use crate::types::repr::TemplateType;
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let mut next = 1u32;
        let mut memo = FxHashMap::default();

        let template = |interner: &mut Interner, hole: TypeId| {
            interner.intern_template(TemplateType {
                texts: vec!["tag:".to_string(), String::new()],
                holes: vec![hole],
            })
        };

        // `string` hole → symbolic pattern (unchanged). It memoizes to itself via the
        // SetMemo discipline (backlog 55) — idempotent, mirroring a conditional whose
        // concrete operands stay undecidable.
        let pattern = template(&mut interner, wk.string);
        assert_eq!(
            eval(&mut interner, &mut next, &mut memo, pattern),
            pattern,
            "a `${{string}}` pattern stays symbolic"
        );
        assert_eq!(
            memo.get(&pattern).copied(),
            Some(pattern),
            "a symbolic template memoizes to itself (idempotent)"
        );

        // Free type parameter hole → deferred (symbolic).
        let t = interner.intern_type_param(TypeParamId(0), "T");
        let deferred = template(&mut interner, t);
        assert_eq!(
            eval(&mut interner, &mut next, &mut memo, deferred),
            deferred
        );

        // Error hole → error type (M22 cascade suppression).
        let err = template(&mut interner, wk.error);
        assert_eq!(
            eval(&mut interner, &mut next, &mut memo, err),
            wk.error,
            "an error-typed hole degrades the template to the error type"
        );
    }

    /// M26 — a mapped type over a **free** declaration type parameter stays deferred: the
    /// evaluator returns the node unchanged (related conservatively by the M25 model),
    /// and it is NOT memoized.
    #[test]
    fn deferred_mapped_over_free_param_is_returned_unchanged() {
        use crate::types::repr::{MappedType, ModifierOp};
        let mut interner = Interner::with_intrinsics();
        let t = interner.intern_type_param(TypeParamId(0), "T");
        let placeholder = interner.intern_mapped_value();
        let mapped = interner.intern_mapped(MappedType {
            homomorphic: true,
            key_source: t, // a free parameter → deferred
            value_template: placeholder,
            modifiers_source: None,
            optional_modifier: ModifierOp::Keep,
            readonly_modifier: ModifierOp::Keep,
        });
        let mut next = 1u32;
        let mut memo = FxHashMap::default();
        let result = eval(&mut interner, &mut next, &mut memo, mapped);
        assert_eq!(
            result, mapped,
            "a deferred mapped type is returned unchanged"
        );
        assert!(
            !memo.contains_key(&mapped),
            "a deferred mapped type is not memoized"
        );
    }

    /// M28 — a **deferred `keyof`** over a free type parameter is returned unchanged
    /// (and not memoized); once its operand is concrete (an object) it resolves
    /// through the SHARED keyof computation to the key-literal union; an error
    /// operand degrades to the error type; a concrete-but-non-object operand (a
    /// primitive after substitution) stays a deferred node — never a permissive
    /// fallback.
    #[test]
    fn deferred_keyof_defers_and_resolves_via_shared_computation() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let mut next = 1u32;
        let mut memo = FxHashMap::default();

        // Free operand: unchanged, un-memoized.
        let t = interner.intern_type_param(TypeParamId(0), "T");
        let keyof_t = interner.intern_keyof(t);
        assert_eq!(eval(&mut interner, &mut next, &mut memo, keyof_t), keyof_t);
        assert!(
            !memo.contains_key(&keyof_t),
            "a deferred keyof is not memoized"
        );

        // Concrete object operand: the key-literal union (same as the eager path).
        let obj = interner.intern_object(ObjectType {
            properties: vec![prop("a", wk.number), prop("b", wk.string)],
            ..Default::default()
        });
        let keyof_obj = interner.intern_keyof(obj);
        let a = interner.intern_literal(LiteralValue::String("a".into()));
        let b = interner.intern_literal(LiteralValue::String("b".into()));
        let expect = interner.union(vec![a, b]);
        assert_eq!(eval(&mut interner, &mut next, &mut memo, keyof_obj), expect);
        let eager = keyof_of_object(&mut interner, obj).expect("object operand keys");
        assert_eq!(
            eager, expect,
            "single source of truth: eager == deferred result"
        );

        // Error operand: the error type (M22 cascade suppression).
        let keyof_err = interner.intern_keyof(wk.error);
        assert_eq!(
            eval(&mut interner, &mut next, &mut memo, keyof_err),
            wk.error
        );

        // Concrete non-object operand: stays deferred (conservative, not permissive).
        let keyof_num = interner.intern_keyof(wk.number);
        assert_eq!(
            eval(&mut interner, &mut next, &mut memo, keyof_num),
            keyof_num
        );
    }

    /// M28 string intrinsics: literals transform, unions distribute, and a
    /// non-literal argument stays a symbolic instantiation.
    #[test]
    fn string_intrinsics_transform_distribute_and_stay_symbolic() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let mut next = 1u32;
        let mut memo = FxHashMap::default();
        let s_param = TypeParamId(0);

        let lit = |interner: &mut Interner, v: &str| {
            interner.intern_literal(LiteralValue::String(v.to_string()))
        };
        let apply = |interner: &mut Interner,
                     next: &mut u32,
                     memo: &mut FxHashMap<TypeId, TypeId>,
                     base: TypeId,
                     arg: TypeId| {
            let inst = interner.intern_instantiation(base, vec![(s_param, arg)]);
            let mut ev = ConditionalEvaluator::new(interner, next, memo, DEFAULT_STEP_BUDGET);
            ev.evaluate(inst)
        };

        // Literal transforms — the four kinds.
        let abc = lit(&mut interner, "abc");
        let big = lit(&mut interner, "ABC");
        let cases = [
            (wk.uppercase, abc, "ABC"),
            (wk.lowercase, big, "abc"),
            (wk.capitalize, abc, "Abc"),
            (wk.uncapitalize, big, "aBC"),
        ];
        for (base, arg, expect) in cases {
            let expect = lit(&mut interner, expect);
            assert_eq!(
                apply(&mut interner, &mut next, &mut memo, base, arg),
                expect
            );
        }
        // The empty string is unchanged (no first char to map).
        let empty = lit(&mut interner, "");
        assert_eq!(
            apply(&mut interner, &mut next, &mut memo, wk.capitalize, empty),
            empty
        );

        // A union argument distributes per member.
        let a = lit(&mut interner, "a");
        let b = lit(&mut interner, "b");
        let ab = interner.union(vec![a, b]);
        let big_a = lit(&mut interner, "A");
        let big_b = lit(&mut interner, "B");
        let expect = interner.union(vec![big_a, big_b]);
        assert_eq!(
            apply(&mut interner, &mut next, &mut memo, wk.uppercase, ab),
            expect
        );

        // A non-literal argument stays the symbolic (identical, hash-consed) node.
        let sym = interner.intern_instantiation(wk.uppercase, vec![(s_param, wk.string)]);
        assert_eq!(eval(&mut interner, &mut next, &mut memo, sym), sym);
    }

    /// M28 — a non-homomorphic map with a **modifiers source** (the `Pick` shape)
    /// resolves each key against the source object: the property's value type
    /// replaces the placeholder and its `?` flag survives (with the M21
    /// `| undefined` baked in); a key the source lacks keeps the M26 behavior
    /// (error-typed value, flags absent).
    #[test]
    fn modifiers_source_preserves_values_and_flags() {
        use crate::types::repr::{MappedType, ModifierOp};
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let placeholder = interner.intern_mapped_value();

        // Source `{ a: number; b?: string }` (M21 stores b as `string | undefined`).
        let str_or_undef = interner.union(vec![wk.string, wk.undefined]);
        let mut b = prop("b", str_or_undef);
        b.optional = true;
        let source = interner.intern_object(ObjectType {
            properties: vec![prop("a", wk.number), b],
            ..Default::default()
        });

        // `{ [P in "a" | "b" | "q"]: T[P] }` with modifiers source = the object.
        let a_key = interner.intern_literal(LiteralValue::String("a".into()));
        let b_key = interner.intern_literal(LiteralValue::String("b".into()));
        let q_key = interner.intern_literal(LiteralValue::String("q".into()));
        let keys = interner.union(vec![a_key, b_key, q_key]);
        let mapped = interner.intern_mapped(MappedType {
            homomorphic: false,
            key_source: keys,
            value_template: placeholder,
            modifiers_source: Some(source),
            optional_modifier: ModifierOp::Keep,
            readonly_modifier: ModifierOp::Keep,
        });
        let mut next = 0u32;
        let mut memo = FxHashMap::default();
        let result = eval(&mut interner, &mut next, &mut memo, mapped);

        let props: Vec<PropertyType> = interner
            .store()
            .object_type(result)
            .expect("result is an object")
            .properties
            .clone();
        let get = |name: &str| {
            props
                .iter()
                .find(|p| p.name == name)
                .unwrap_or_else(|| panic!("property {name} present"))
                .clone()
        };
        assert_eq!(get("a").ty, wk.number, "picked value type preserved");
        assert!(!get("a").optional);
        assert!(get("b").optional, "picked optionality preserved");
        assert_eq!(get("b").ty, str_or_undef);
        assert!(!get("q").optional, "a missing key keeps the M26 defaults");
        assert_eq!(get("q").ty, wk.error);
    }

    /// M26 — `-?` Required semantics (probed tsc 6.0.3, leader-arbitrated): over an
    /// **optional** source member, `undefined` is stripped from the **evaluated** value
    /// type — including a template-re-added `| undefined`; a result that is EXACTLY
    /// `undefined` maps to `never`; a **non-optional** source member never strips
    /// (template-added `undefined` is kept).
    #[test]
    fn required_strips_undefined_from_optional_source_values() {
        use crate::types::repr::{MappedType, ModifierOp};
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let placeholder = interner.intern_mapped_value();

        // Source `{ a: string | undefined; b?: string; u?: undefined }` — M21 stores an
        // optional member's effective type with `| undefined` baked in.
        let str_or_undef = interner.union(vec![wk.string, wk.undefined]);
        let mut b = prop("b", str_or_undef);
        b.optional = true;
        let mut u = prop("u", wk.undefined);
        u.optional = true;
        let source = interner.intern_object(ObjectType {
            properties: vec![prop("a", str_or_undef), b, u],
            ..Default::default()
        });

        // `{ [K in keyof T]-?: T[K] | undefined }` — the template RE-ADDS `undefined`,
        // distinguishing a result-level strip from a source-level one.
        let template = interner.union(vec![placeholder, wk.undefined]);
        let req = interner.intern_mapped(MappedType {
            homomorphic: true,
            key_source: source,
            value_template: template,
            modifiers_source: None,
            optional_modifier: ModifierOp::Remove,
            readonly_modifier: ModifierOp::Keep,
        });
        let mut next = 0u32;
        let mut memo = FxHashMap::default();
        let result = eval(&mut interner, &mut next, &mut memo, req);

        let props: Vec<PropertyType> = interner
            .store()
            .object_type(result)
            .expect("result is an object")
            .properties
            .clone();
        let get = |name: &str| {
            props
                .iter()
                .find(|p| p.name == name)
                .unwrap_or_else(|| panic!("property {name} present"))
                .clone()
        };
        // Optional source `b`: undefined stripped from the whole RESULT (even the
        // template-added one) → exactly `string`, and required.
        let b_out = get("b");
        assert!(!b_out.optional, "-? clears optionality");
        assert_eq!(
            b_out.ty, wk.string,
            "undefined stripped from the evaluated value"
        );
        // Exactly-undefined optional source `u`: maps to `never` (leader-arbitrated
        // tsc probe m26_arb.ts — filtering `undefined` by not-undefined leaves nothing).
        assert_eq!(
            get("u").ty,
            wk.never,
            "an exactly-undefined value maps to never"
        );
        // NON-optional source `a`: never strips — keeps `string | undefined`.
        assert_eq!(
            get("a").ty,
            str_or_undef,
            "a non-optional source member keeps its undefined"
        );
    }
}

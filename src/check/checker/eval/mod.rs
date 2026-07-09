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

mod extends;
mod instantiation;
mod keyof;
mod mapped;
mod template;
#[cfg(test)]
mod tests;

pub(in crate::check) use extends::evaluate_inference_constraint;
pub(in crate::check) use keyof::{contains_deferred_argument, contains_deferred_keyof};
pub(in crate::check::checker) use keyof::keyof_of_type;

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
}


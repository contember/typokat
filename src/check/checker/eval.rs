//! The conditional-type **evaluator** (M25, architecture §7 — the type-level
//! evaluation phase's first slice).
//!
//! A conditional `C extends E ? T : F` is *evaluated* once its check type `C` is
//! **concrete** (contains no free declaration type parameter). Evaluation:
//!
//!  1. **Distribution** (of a distributive conditional applied via an
//!     [`crate::types::repr::InstantiationType`]): if the check argument is a union it
//!     distributes over its members, `never` distributes to zero members (→ `never`),
//!     and the `boolean` intrinsic expands to `true | false` first.
//!  2. **The `extends` test** runs through the **existing** relation engine
//!     ([`Relater`]); `infer` binders are extracted through the **existing** inference
//!     machinery in its conditional collection mode
//!     ([`infer_from_types_for_conditional`] — literals never widen, union extends
//!     targets descend) after freshening the node's de Bruijn binders to transient
//!     [`TypeParamId`]s (ADR-0002 — the acceptable fallback recorded in the sprint run
//!     log).
//!  3. **Branch selection**: the true branch (with matched `infer` candidates
//!     substituted) on success, the false branch on failure.
//!
//! ## Required machinery (architecture §7.2)
//!
//!  - **Memoization** keyed on the interned conditional/instantiation `TypeId` → result.
//!    Sound because hash-consing makes the key *total* (one id ⇒ one type ⇒ one result).
//!    A result computed while the per-root step budget is **exhausted**, or one that
//!    re-entered an *in-flight* id (a genuine cycle), is **not** committed to the memo —
//!    mirroring the relation cache's provisional discipline (invariants §1).
//!  - An explicit **work-stack** — no host recursion per evaluation step, so a deeply
//!    recursive `Unwrap`-style descent cannot overflow the native stack (witnessed by
//!    the 10 000-deep unit test below).
//!  - A per-root **step budget** (`~1000`, tsc's tail-iteration order): exhaustion sets
//!    [`ConditionalEvaluator::exhausted`], which the caller turns into `TK2589` at the
//!    annotation that demanded evaluation.

use super::context::Pass;
use crate::check::infer::infer_from_types_for_conditional;
use crate::diagnostics::Diagnostic;
use crate::relate::Relater;
use crate::span::Span;
use crate::types::repr::{ConditionalType, LiteralValue, TypeParamId, TypeTag};
use crate::types::store::TypeId;
use crate::types::{substitute, Interner};
use rustc_hash::{FxHashMap, FxHashSet};

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Evaluate a lowered type at a **value-position demand site** (M25): a directly
    /// concrete conditional, a lazy alias instantiation, or a union of those resolves to
    /// its result; a deferred conditional (or any other type) is returned unchanged. On
    /// a per-root instantiation-depth blow-out the evaluator sets `exhausted`, which is
    /// reported as `TK2589` at `span` (the annotation that demanded evaluation — a
    /// documented span divergence from tsc).
    ///
    /// Cheap-rejects everything that is not itself a conditional / instantiation / union,
    /// so ordinary annotations pay only a tag check. Called after alias/generic
    /// instantiation, at a bare conditional-alias reference, at an inline conditional
    /// annotation, and on a generic call's substituted return type.
    pub(in crate::check::checker) fn evaluate_type(&mut self, ty: TypeId, span: Span) -> TypeId {
        if !matches!(
            self.interner.store().tag(ty),
            TypeTag::Conditional | TypeTag::Instantiation | TypeTag::Union
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
        let (matched, true_final) = self.run_extends_test(&cond);
        let branch = if matched {
            true_final
        } else {
            cond.false_branch
        };
        // The result IS the evaluated branch: memoize this id to it once the branch
        // resolves (tail step — a chain of conditionals is a loop here).
        tasks.push(Task::SetMemo(ty));
        tasks.push(Task::Eval(branch));
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
        self.in_flight.insert(ty);

        let per_conditionals = self.expand_instantiation(&inst);

        tasks.push(Task::SetMemo(ty));
        tasks.push(Task::BuildUnion(per_conditionals.len()));
        // Push in reverse so they are evaluated (popped) in order — union is
        // order-independent, but this keeps behaviour deterministic.
        for &cond in per_conditionals.iter().rev() {
            tasks.push(Task::Eval(cond));
        }
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
                TypeTag::Conditional | TypeTag::Instantiation
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
            .map(|&t| self.interner.store().type_param(t).map(|p| p.id).unwrap_or(TypeParamId(0)))
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
            TypeTag::Intrinsic | TypeTag::Literal | TypeTag::TypeParam | TypeTag::Conditional => ty,
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
                let Some(elements) = self.interner.store().tuple_type(ty).map(|t| t.elements.clone())
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
                    TypeTag::Intrinsic | TypeTag::Literal | TypeTag::Infer => {
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
            TypeTag::Array => store
                .array_type(ty)
                .map(|a| vec![a.element])
                .unwrap_or_default(),
            TypeTag::Tuple => store
                .tuple_type(ty)
                .map(|t| t.elements.clone())
                .unwrap_or_default(),
            TypeTag::Conditional => store
                .conditional_type(ty)
                .map(|c| vec![c.check, c.extends_ty, c.true_branch, c.false_branch])
                .unwrap_or_default(),
            TypeTag::Instantiation => store
                .instantiation_type(ty)
                .map(|i| i.args.iter().map(|(_, v)| *v).collect())
                .unwrap_or_default(),
            _ => Vec::new(),
        }
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
        assert_eq!(result, wk.number, "Unwrap fully descends to the innermost `number`");
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
        let mut ev = ConditionalEvaluator::new(&mut interner, &mut next_type_param, &mut memo, DEFAULT_STEP_BUDGET);
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
        let mut ev = ConditionalEvaluator::new(&mut interner, &mut next_type_param, &mut memo, DEFAULT_STEP_BUDGET);
        let _ = ev.evaluate(root);
        assert!(ev.exhausted, "a runaway alias must exhaust the step budget");
    }
}

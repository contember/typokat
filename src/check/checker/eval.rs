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
use crate::types::repr::{
    ConditionalType, LiteralValue, MappedType, ModifierOp, ObjectType, PropertyType, TemplateType,
    TypeParamId, TypeTag,
};
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
            TypeTag::Conditional
                | TypeTag::Instantiation
                | TypeTag::Union
                | TypeTag::Mapped
                | TypeTag::Template
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
                        TypeTag::Template => self.eval_template(ty, &mut values, error),
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
                TypeTag::Conditional | TypeTag::Instantiation | TypeTag::Mapped | TypeTag::Template
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

    /// Schedule the evaluation of a mapped type `ty` (M26). A mapped type over a **free**
    /// key source is deferred (returned as its own value, related conservatively by the
    /// M25 model); a concrete one first evaluates its key source (a tail step, so a
    /// mapped-of-mapped is a loop, not host recursion), then [`Task::AssembleMapped`]
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
        // resolves and the properties are built (tail steps).
        tasks.push(Task::SetMemo(ty));
        tasks.push(Task::AssembleMapped(ty));
        tasks.push(Task::Eval(mapped.key_source));
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
    /// `exhausted`), never OOM. The result is memoized (a concrete collapse only — a
    /// symbolic template is idempotent and left un-memoized, mirroring a deferred
    /// conditional).
    fn eval_template(&mut self, ty: TypeId, values: &mut Vec<TypeId>, error: TypeId) {
        let Some(template) = self.interner.store().template_type(ty).cloned() else {
            values.push(ty);
            return;
        };
        let wk = self.interner.well_known();

        // M22 discipline: an error-typed hole (an unresolved name upstream) degrades the
        // whole template to the error type so cascades stay suppressed — mirroring
        // `assemble_mapped`'s error/any key-source handling.
        if template.holes.contains(&wk.error) {
            self.memo.insert(ty, error);
            values.push(error);
            return;
        }

        // Classify each hole; a `never` hole makes the whole template `never`, a
        // non-literal hole keeps it symbolic, otherwise it is a cartesian factor.
        let mut factors: Vec<Vec<String>> = Vec::with_capacity(template.holes.len());
        for &hole in &template.holes {
            match self.hole_parts(hole) {
                HolePart::Never => {
                    self.memo.insert(ty, wk.never);
                    values.push(wk.never);
                    return;
                }
                HolePart::NonLiteral => {
                    // A symbolic pattern (string/number intrinsic, free param, …) —
                    // return the node unchanged, un-memoized (idempotent).
                    values.push(ty);
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
        self.memo.insert(ty, result);
        values.push(result);
    }

    /// Classify a template hole for construction (M27). A string/number/boolean literal
    /// (or a union thereof) yields the ordered list of string parts it contributes to the
    /// cartesian product; the `boolean` intrinsic expands to `"false"`/`"true"`; the
    /// `never` intrinsic short-circuits the whole template; anything else (a
    /// `string`/`number` intrinsic, a free parameter, an `infer` binder, or a union with
    /// any non-literal member) leaves the template symbolic.
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
            values.push(ty);
            return;
        };
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
            // source. No source property flags → the modifier arithmetic starts absent
            // (and there is no optional source to strip `undefined` for). A key set with
            // any non-string-literal member (`K in string`, a numeric key) is out of
            // subset → deferred.
            let Some(names) = self.literal_string_keys(key_source) else {
                values.push(ty);
                return;
            };
            for name in names {
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

        tasks.push(Task::BuildMappedObject(meta));
        // Push in reverse so the per-property values pop (and their results land) in
        // order, aligning with the metadata order in `BuildMappedObject`.
        for &v in value_pre.iter().rev() {
            tasks.push(Task::Eval(v));
        }
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
    ///  - an object **with** index signatures (no `K in string` production), a
    ///    primitive, or any other shape → `None`.
    fn homomorphic_source_props(&mut self, key_source: TypeId) -> Option<Vec<PropertyType>> {
        let store = self.interner.store();
        if let Some(object) = store.object_type(key_source) {
            if object.string_index.is_some() || object.number_index.is_some() {
                return None;
            }
            return Some(object.properties.clone());
        }
        let members = store.union_members(key_source)?.to_vec();
        let mut member_objects: Vec<Vec<PropertyType>> = Vec::with_capacity(members.len());
        for member in &members {
            let object = store.object_type(*member)?;
            if object.string_index.is_some() || object.number_index.is_some() {
                return None;
            }
            member_objects.push(object.properties.clone());
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

    /// Build the result object of a mapped evaluation (M26): pair each property's
    /// metadata with its evaluated value type (by position), stripping `undefined` for a
    /// `-?`-over-optional property (tsc `Required` semantics) and baking `| undefined`
    /// into an **optional** property's stored type (the M21 convention the relation
    /// engine reads back), then interning the object.
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
            Some(members) if members.contains(&wk.undefined) => {
                members.iter().copied().filter(|&m| m != wk.undefined).collect()
            }
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
                if key_source == mapped.key_source {
                    return ty;
                }
                self.interner.intern_mapped(MappedType {
                    key_source,
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
                let Some(elements) =
                    self.interner.store().tuple_type(ty).map(|t| t.elements.clone())
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
            // M26: a mapped type is concrete once its key source and value template are
            // (the value template's `MappedValue` placeholder is a bound variable —
            // classified concrete above).
            TypeTag::Mapped => store
                .mapped_type(ty)
                .map(|m| vec![m.key_source, m.value_template])
                .unwrap_or_default(),
            // M27: a template is concrete once every hole is (a free type parameter hole,
            // e.g. `` `tag:${T}` ``, makes it a deferred node).
            TypeTag::Template => store
                .template_type(ty)
                .map(|t| t.holes.clone())
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

    /// M26 — a homomorphic identity mapped type `{ [K in keyof T]: T[K] }` over a
    /// concrete source evaluates to the source's shape (per-property `T[K]` = the source
    /// property's type), and its result is memoized.
    fn eval(interner: &mut Interner, next: &mut u32, memo: &mut FxHashMap<TypeId, TypeId>, ty: TypeId) -> TypeId {
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
        assert!(memo.contains_key(&ident), "the mapped evaluation is memoized");
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
        assert_eq!(a.ty, expected, "value template `T[K] | null` + optional `| undefined`");
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

        // `string` hole → symbolic pattern (unchanged, un-memoized).
        let pattern = template(&mut interner, wk.string);
        assert_eq!(
            eval(&mut interner, &mut next, &mut memo, pattern),
            pattern,
            "a `${{string}}` pattern stays symbolic"
        );
        assert!(!memo.contains_key(&pattern), "a symbolic template is not memoized");

        // Free type parameter hole → deferred (symbolic).
        let t = interner.intern_type_param(TypeParamId(0), "T");
        let deferred = template(&mut interner, t);
        assert_eq!(eval(&mut interner, &mut next, &mut memo, deferred), deferred);

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
            optional_modifier: ModifierOp::Keep,
            readonly_modifier: ModifierOp::Keep,
        });
        let mut next = 1u32;
        let mut memo = FxHashMap::default();
        let result = eval(&mut interner, &mut next, &mut memo, mapped);
        assert_eq!(result, mapped, "a deferred mapped type is returned unchanged");
        assert!(
            !memo.contains_key(&mapped),
            "a deferred mapped type is not memoized"
        );
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
        assert_eq!(b_out.ty, wk.string, "undefined stripped from the evaluated value");
        // Exactly-undefined optional source `u`: maps to `never` (leader-arbitrated
        // tsc probe m26_arb.ts — filtering `undefined` by not-undefined leaves nothing).
        assert_eq!(get("u").ty, wk.never, "an exactly-undefined value maps to never");
        // NON-optional source `a`: never strips — keeps `string | undefined`.
        assert_eq!(
            get("a").ty,
            str_or_undef,
            "a non-optional source member keeps its undefined"
        );
    }
}

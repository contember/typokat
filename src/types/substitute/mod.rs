//! Type-parameter substitution for generic instantiation.
//! Rewrites free declaration parameters through composite types and re-interns the
//! result, so equal instantiations share one `TypeId`. The `in_progress` guard
//! returns the original id on self-referential nominal types; recursive generic
//! instantiation remains out of scope.

use crate::types::repr::{FunctionType, TypeParamId, TypeTag};
use crate::types::store::{Store, TypeId};
use crate::types::Interner;
use rustc_hash::{FxHashMap, FxHashSet};

#[cfg(test)]
#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct SubstitutionMeasure {
    pub runs: u64,
    pub apply_visits: u64,
    pub type_id_repeats: u64,
    pub exact_context_repeats: u64,
    pub type_param_map_hits: u64,
    pub blocked_type_param_hits: u64,
    pub cycle_reentries: u64,
    pub completed_memo_hits: u64,
    pub completed_memo_entries: u64,
    pub cycle_tainted_skips: u64,
    pub tainted_memo_entries: u64,
    pub tainted_memo_hits: u64,
    pub tainted_memo_stale_misses: u64,
    pub prefilter_skips: u64,
    pub prefilter_graph_scans: u64,
    seen_type_ids: FxHashSet<TypeId>,
    seen_contexts: FxHashSet<(TypeId, Vec<TypeParamId>)>,
}

#[cfg(test)]
impl std::fmt::Debug for SubstitutionMeasure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubstitutionMeasure")
            .field("runs", &self.runs)
            .field("apply_visits", &self.apply_visits)
            .field("type_id_repeats", &self.type_id_repeats)
            .field("exact_context_repeats", &self.exact_context_repeats)
            .field("type_param_map_hits", &self.type_param_map_hits)
            .field("blocked_type_param_hits", &self.blocked_type_param_hits)
            .field("cycle_reentries", &self.cycle_reentries)
            .field("completed_memo_hits", &self.completed_memo_hits)
            .field("completed_memo_entries", &self.completed_memo_entries)
            .field("cycle_tainted_skips", &self.cycle_tainted_skips)
            .field("tainted_memo_entries", &self.tainted_memo_entries)
            .field("tainted_memo_hits", &self.tainted_memo_hits)
            .field("tainted_memo_stale_misses", &self.tainted_memo_stale_misses)
            .field("prefilter_skips", &self.prefilter_skips)
            .field("prefilter_graph_scans", &self.prefilter_graph_scans)
            .finish()
    }
}

#[cfg(test)]
thread_local! {
    static SUBSTITUTION_MEASURE: std::cell::RefCell<Option<SubstitutionMeasureCollector>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
type SubstitutionMeasureCollector = std::rc::Rc<std::cell::RefCell<SubstitutionMeasure>>;

#[cfg(test)]
pub(super) struct SubstitutionMeasureScope {
    previous: Option<SubstitutionMeasureCollector>,
    _thread_affine: std::marker::PhantomData<std::rc::Rc<()>>,
}

#[cfg(test)]
impl Drop for SubstitutionMeasureScope {
    fn drop(&mut self) {
        SUBSTITUTION_MEASURE.with(|measure| {
            measure.replace(self.previous.take());
        });
    }
}

#[cfg(test)]
pub(super) fn start_substitution_measure() -> SubstitutionMeasureScope {
    let collector = std::rc::Rc::new(std::cell::RefCell::new(SubstitutionMeasure::default()));
    let previous =
        SUBSTITUTION_MEASURE.with(|current| current.replace(Some(std::rc::Rc::clone(&collector))));
    SubstitutionMeasureScope {
        previous,
        _thread_affine: std::marker::PhantomData,
    }
}

#[cfg(test)]
pub(super) fn substitution_measure() -> Option<SubstitutionMeasure> {
    let collector = SUBSTITUTION_MEASURE.with(|current| current.borrow().clone())?;
    let measure = collector.borrow().clone();
    Some(measure)
}

#[cfg(test)]
fn capture_substitution_measurement() -> Option<SubstitutionMeasureCollector> {
    let collector = SUBSTITUTION_MEASURE.with(|current| current.borrow().clone());
    if let Some(collector) = collector.as_ref() {
        collector.borrow_mut().runs += 1;
    }
    collector
}

#[cfg(test)]
fn measure_substitution_visit(
    collector: &SubstitutionMeasureCollector,
    ty: TypeId,
    blocked: &FxHashSet<TypeParamId>,
) {
    let mut measure = collector.borrow_mut();
    measure.apply_visits += 1;
    if !measure.seen_type_ids.insert(ty) {
        measure.type_id_repeats += 1;
    }
    let mut context: Vec<TypeParamId> = blocked.iter().copied().collect();
    context.sort_unstable();
    if !measure.seen_contexts.insert((ty, context)) {
        measure.exact_context_repeats += 1;
    }
}

#[cfg(test)]
fn measure_substitution(
    collector: &SubstitutionMeasureCollector,
    update: impl FnOnce(&mut SubstitutionMeasure),
) {
    update(&mut collector.borrow_mut());
}

#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SubstitutionRunVisitMeasure {
    pub(crate) executed_visits: u64,
    pub(crate) completed_memo_hits: u64,
    pub(crate) saturated: bool,
}

#[cfg(test)]
type SubstitutionRunVisitMeasureCollector =
    std::rc::Rc<std::cell::RefCell<SubstitutionRunVisitMeasure>>;

#[cfg(test)]
thread_local! {
    static SUBSTITUTION_RUN_VISIT_MEASURE: std::cell::RefCell<Option<SubstitutionRunVisitMeasureCollector>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(crate) struct SubstitutionRunVisitMeasureScope {
    previous: Option<SubstitutionRunVisitMeasureCollector>,
    _thread_affine: std::marker::PhantomData<std::rc::Rc<()>>,
}

#[cfg(test)]
impl Drop for SubstitutionRunVisitMeasureScope {
    fn drop(&mut self) {
        SUBSTITUTION_RUN_VISIT_MEASURE.with(|measure| {
            measure.replace(self.previous.take());
        });
    }
}

#[cfg(test)]
pub(crate) fn start_substitution_run_visit_measure() -> SubstitutionRunVisitMeasureScope {
    let collector = std::rc::Rc::new(std::cell::RefCell::new(
        SubstitutionRunVisitMeasure::default(),
    ));
    let previous = SUBSTITUTION_RUN_VISIT_MEASURE
        .with(|current| current.replace(Some(std::rc::Rc::clone(&collector))));
    SubstitutionRunVisitMeasureScope {
        previous,
        _thread_affine: std::marker::PhantomData,
    }
}

#[cfg(test)]
pub(crate) fn substitution_run_visit_measure() -> Option<SubstitutionRunVisitMeasure> {
    let collector = SUBSTITUTION_RUN_VISIT_MEASURE.with(|current| current.borrow().clone())?;
    let measure = collector.borrow().clone();
    Some(measure)
}

#[cfg(test)]
fn capture_substitution_run_visit_measure() -> Option<SubstitutionRunVisitMeasureCollector> {
    SUBSTITUTION_RUN_VISIT_MEASURE.with(|current| current.borrow().clone())
}

#[cfg(test)]
fn record_substitution_run_visit(collector: &SubstitutionRunVisitMeasureCollector, memo_hit: bool) {
    let mut measure = collector.borrow_mut();
    measure.executed_visits = match measure.executed_visits.checked_add(1) {
        Some(value) => value,
        None => {
            measure.saturated = true;
            u64::MAX
        }
    };
    if memo_hit {
        measure.completed_memo_hits = match measure.completed_memo_hits.checked_add(1) {
            Some(value) => value,
            None => {
                measure.saturated = true;
                u64::MAX
            }
        };
    }
}

mod apply;
#[cfg(test)]
mod completed_memo_spec;
#[cfg(test)]
mod cycle_scoped_memo_spec;
#[cfg(test)]
mod measurement_scope_spec;
#[cfg(test)]
mod param_relevant_prefilter_spec;
#[cfg(test)]
mod tests;

/// Whether a substitution completed without relying on the raw-id cycle guard.
/// Cycle-tainted results remain valid for the current occurrence but must not be
/// reused by a cache as though they were context-free completed values.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum SubstitutionOutcome {
    CycleClean(TypeId),
    CycleTainted(TypeId),
}

/// A cycle-tainted result plus the stack cut it depends on: reusable only while
/// every `reentered` id is still in progress and no `visited` id became one.
#[derive(Clone)]
struct TaintedEntry {
    result: TypeId,
    reentered: FxHashSet<TypeId>,
    visited: FxHashSet<TypeId>,
}

/// Whether an id of this tag can ever appear in `in_progress` (only these arms
/// insert it). Other tags can never cut a walk, so recording them in `visited`
/// would be dead weight — keep this in sync with the `apply_*` arms.
fn tag_can_reenter(tag: TypeTag) -> bool {
    matches!(
        tag,
        TypeTag::Object | TypeTag::Function | TypeTag::Union | TypeTag::Intersection
    )
}

/// The prefilter tracks free params as a `u64` bitmask; larger maps walk instead.
const PREFILTER_MAX_PARAMS: usize = 64;

fn param_bit(param_keys: &[TypeParamId], id: TypeParamId) -> u64 {
    param_keys
        .binary_search(&id)
        .map_or(0, |index| 1u64 << index)
}

/// Visit exactly the child ids the `apply_*` arms recurse into — keep this in
/// sync with `apply.rs` (like `tag_can_reenter`); the prefilter is only sound
/// while these edges mirror the recursion.
fn for_each_apply_child(
    store: &Store,
    ty: TypeId,
    #[cfg(test)] measurement: Option<&SubstitutionMeasureCollector>,
    mut visit: impl FnMut(TypeId),
) {
    #[cfg(test)]
    if let Some(collector) = measurement {
        collector.borrow_mut().prefilter_graph_scans += 1;
    }
    match store.tag(ty) {
        // Terminal arms: no recursion (a TypeParam's own id is handled by the caller).
        TypeTag::TypeParam
        | TypeTag::Intrinsic
        | TypeTag::Literal
        | TypeTag::Infer
        | TypeTag::MappedValue => {}
        TypeTag::Object => {
            // Unfilled reserved objects have no accessor row → no children
            // (sound: `apply_object` returns them untouched).
            let Some(object) = store.object_type(ty) else {
                return;
            };
            for property in &object.properties {
                visit(property.ty);
                if let Some(write_ty) = property.write_ty {
                    visit(write_ty);
                }
            }
            if let Some(index) = object.string_index {
                visit(index);
            }
            if let Some(index) = object.number_index {
                visit(index);
            }
            for &signature in object
                .call_signatures
                .iter()
                .chain(&object.construct_signatures)
            {
                visit(signature);
            }
        }
        TypeTag::Function => {
            let Some(function) = store.function_type(ty) else {
                return;
            };
            for param in &function.type_params {
                if let Some(constraint) = param.constraint {
                    visit(constraint);
                }
                if let Some(default) = param.default {
                    visit(default);
                }
            }
            for param in &function.params {
                visit(param.ty);
            }
            if let Some(receiver) = function.receiver {
                visit(receiver);
            }
            visit(function.ret);
        }
        TypeTag::Union => {
            for &member in store.union_members(ty).unwrap_or(&[]) {
                visit(member);
            }
        }
        TypeTag::Intersection => {
            for &member in store.intersection_members(ty).unwrap_or(&[]) {
                visit(member);
            }
        }
        TypeTag::Array => {
            if let Some(array) = store.array_type(ty) {
                visit(array.element);
            }
        }
        TypeTag::Tuple => {
            let Some(tuple) = store.tuple_type(ty) else {
                return;
            };
            for &element in &tuple.elements {
                visit(element);
            }
            if let Some(rest) = &tuple.rest {
                visit(rest.ty);
            }
        }
        TypeTag::Readonly => {
            if let Some(operand) = store.readonly_operand(ty) {
                visit(operand);
            }
        }
        TypeTag::Keyof => {
            if let Some(operand) = store.keyof_operand(ty) {
                visit(operand);
            }
        }
        TypeTag::DeferredIndexedAccess => {
            if let Some(access) = store.deferred_indexed_access_type(ty) {
                visit(access.object);
                visit(access.index);
            }
        }
        TypeTag::Template => {
            if let Some(template) = store.template_type(ty) {
                for &hole in &template.holes {
                    visit(hole);
                }
            }
        }
        TypeTag::Conditional => {
            if let Some(cond) = store.conditional_type(ty) {
                visit(cond.check);
                visit(cond.extends_ty);
                visit(cond.true_branch);
                visit(cond.false_branch);
            }
        }
        TypeTag::Instantiation => {
            // `apply_instantiation` composes into argument VALUES only — never the base.
            if let Some(inst) = store.instantiation_type(ty) {
                for &(_, value) in &inst.args {
                    visit(value);
                }
            }
        }
        TypeTag::ClassInstance => {
            if let Some(instance) = store.class_instance_type(ty) {
                for &arg in &instance.args {
                    visit(arg);
                }
            }
        }
        TypeTag::Mapped => {
            if let Some(mapped) = store.mapped_type(ty) {
                visit(mapped.key_source);
                visit(mapped.value_template);
                if let Some(modifiers_source) = mapped.modifiers_source {
                    visit(modifiers_source);
                }
            }
        }
    }
}

/// One iterative-Tarjan DFS frame of [`compute_free_sets`].
struct FreeSetFrame {
    ty: TypeId,
    children: Vec<TypeId>,
    next_child: usize,
    /// Bits of this node's own declared binders (Function only), masked at frame end.
    binder_mask: u64,
    bits: u64,
    index: usize,
    lowlink: usize,
}

/// Fill `free_sets` for every node reachable from `root` along the apply-child
/// edges and return `root`'s mask: which `param_keys` occur free (a Function
/// removes its own binder bits from its subtree; SCC members share their union,
/// a sound over-approximation for binders inside a cycle). Iterative Tarjan —
/// recursive DFS would risk stack overflow on deep graphs.
fn compute_free_sets(
    store: &Store,
    param_keys: &[TypeParamId],
    free_sets: &mut FxHashMap<TypeId, u64>,
    #[cfg(test)] measurement: Option<&SubstitutionMeasureCollector>,
    root: TypeId,
) -> u64 {
    debug_assert!(param_keys.len() <= PREFILTER_MAX_PARAMS);
    let push_frame = |ty: TypeId, index: usize| -> FreeSetFrame {
        let mut children = Vec::new();
        for_each_apply_child(
            store,
            ty,
            #[cfg(test)]
            measurement,
            |child| children.push(child),
        );
        let binder_mask = store.function_type(ty).map_or(0, |function| {
            function
                .type_params
                .iter()
                .fold(0, |mask, param| mask | param_bit(param_keys, param.id))
        });
        // A TypeParam contributes its own bit iff its id is a map key.
        let bits = store
            .type_param(ty)
            .map_or(0, |param| param_bit(param_keys, param.id));
        FreeSetFrame {
            ty,
            children,
            next_child: 0,
            binder_mask,
            bits,
            index,
            lowlink: index,
        }
    };

    let mut next_index = 0_usize;
    let mut frames: Vec<FreeSetFrame> = Vec::new();
    let mut index_of: FxHashMap<TypeId, usize> = FxHashMap::default();
    // Nodes discovered in this pass but not yet assigned their SCC union: the
    // Tarjan stack plus each one's partial (post-binder-mask) contribution.
    let mut scc_stack: Vec<TypeId> = Vec::new();
    let mut on_stack: FxHashSet<TypeId> = FxHashSet::default();
    let mut pending_bits: FxHashMap<TypeId, u64> = FxHashMap::default();

    frames.push(push_frame(root, next_index));
    index_of.insert(root, next_index);
    next_index += 1;
    scc_stack.push(root);
    on_stack.insert(root);

    while let Some(frame) = frames.last_mut() {
        if let Some(&child) = frame.children.get(frame.next_child) {
            frame.next_child += 1;
            if let Some(&bits) = free_sets.get(&child) {
                // Completed (this pass or an earlier one): final value.
                frame.bits |= bits;
            } else if let Some(&child_index) = index_of.get(&child) {
                // In-stack back edge: the child is in this frame's own SCC
                // (Tarjan invariant), so its partial contribution — already in
                // `pending_bits` if its frame closed — is fixed up by the
                // SCC-completion union, never taken from a cached final value.
                debug_assert!(on_stack.contains(&child));
                frame.lowlink = frame.lowlink.min(child_index);
                frame.bits |= pending_bits.get(&child).copied().unwrap_or(0);
            } else {
                index_of.insert(child, next_index);
                scc_stack.push(child);
                on_stack.insert(child);
                frames.push(push_frame(child, next_index));
                next_index += 1;
            }
            continue;
        }

        let finished = frames.pop().expect("the loop condition saw a frame");
        let contribution = (finished.bits) & !finished.binder_mask;
        pending_bits.insert(finished.ty, contribution);
        if finished.lowlink == finished.index {
            // SCC root: every member gets the union over ALL members' contributions.
            let first_member = scc_stack
                .iter()
                .rposition(|&member| member == finished.ty)
                .expect("an unfinished node stays on the Tarjan stack");
            let members = scc_stack.split_off(first_member);
            let union = members.iter().fold(0, |union, member| {
                union | pending_bits.get(member).copied().unwrap_or(0)
            });
            for member in members {
                on_stack.remove(&member);
                pending_bits.remove(&member);
                free_sets.insert(member, union);
            }
        }
        if let Some(parent) = frames.last_mut() {
            parent.lowlink = parent.lowlink.min(finished.lowlink);
            // Finalized child → its SCC union; still-pending child → its partial.
            parent.bits |= free_sets
                .get(&finished.ty)
                .or_else(|| pending_bits.get(&finished.ty))
                .copied()
                .unwrap_or(0);
        }
    }

    free_sets
        .get(&root)
        .copied()
        .expect("the pass assigns every reachable node, including the root")
}

/// Compute exact free map-parameter sets as a least fixed point over type ids.
/// Function binders kill their bits at the function boundary, so cycles never
/// multiply states by combinations of binders crossed along different paths.
fn compute_exact_free_sets(
    store: &Store,
    param_keys: &[TypeParamId],
    exact_free_sets: &mut FxHashMap<TypeId, u64>,
    #[cfg(test)] measurement: Option<&SubstitutionMeasureCollector>,
    root: TypeId,
) -> u64 {
    if let Some(&bits) = exact_free_sets.get(&root) {
        return bits;
    }

    let mut pending = vec![root];
    let mut discovered = FxHashSet::default();
    discovered.insert(root);
    let mut parents: FxHashMap<TypeId, Vec<TypeId>> = FxHashMap::default();
    let mut binder_masks: FxHashMap<TypeId, u64> = FxHashMap::default();
    let mut values: FxHashMap<TypeId, u64> = FxHashMap::default();

    while let Some(ty) = pending.pop() {
        if let Some(&cached) = exact_free_sets.get(&ty) {
            values.insert(ty, cached);
            continue;
        }

        values.insert(
            ty,
            store
                .type_param(ty)
                .map_or(0, |param| param_bit(param_keys, param.id)),
        );
        let binder_mask = store.function_type(ty).map_or(0, |function| {
            function
                .type_params
                .iter()
                .fold(0, |mask, param| mask | param_bit(param_keys, param.id))
        });
        binder_masks.insert(ty, binder_mask);
        for_each_apply_child(
            store,
            ty,
            #[cfg(test)]
            measurement,
            |child| {
                parents.entry(child).or_default().push(ty);
                if discovered.insert(child) {
                    pending.push(child);
                }
            },
        );
    }

    let mut deltas = values.clone();
    let mut propagate: Vec<TypeId> = deltas
        .iter()
        .filter_map(|(&ty, &bits)| (bits != 0).then_some(ty))
        .collect();
    let mut queued: FxHashSet<TypeId> = propagate.iter().copied().collect();
    while let Some(ty) = propagate.pop() {
        queued.remove(&ty);
        let delta = deltas.insert(ty, 0).unwrap_or(0);
        if let Some(type_parents) = parents.get(&ty) {
            for &parent in type_parents {
                let contribution = delta & !binder_masks.get(&parent).copied().unwrap_or(0);
                let parent_value = values.entry(parent).or_default();
                let new_bits = contribution & !*parent_value;
                if new_bits != 0 {
                    *parent_value |= new_bits;
                    *deltas.entry(parent).or_default() |= new_bits;
                    if queued.insert(parent) {
                        propagate.push(parent);
                    }
                }
            }
        }
    }

    for ty in discovered {
        exact_free_sets.insert(ty, values.get(&ty).copied().unwrap_or(0));
    }
    exact_free_sets
        .get(&root)
        .copied()
        .expect("the fixed point assigns the root")
}

/// A substitution `TypeParamId → TypeId` plus the cycle guard, applied over the
/// type store. Built once per instantiation and dropped after.
pub struct Substitution<'a> {
    map: &'a FxHashMap<TypeParamId, TypeId>,
    /// Type ids currently being rewritten on the recursion stack — re-entry
    /// returns the original id, breaking a (nominal) cycle. See the module docs.
    in_progress: FxHashSet<TypeId>,
    /// Generic function binders currently crossed while applying an outer
    /// substitution. They shadow matching ids in `map` until that signature exits.
    blocked: FxHashSet<TypeParamId>,
    /// Results completed without observing a recursive re-entry, scoped to this run.
    completed: FxHashMap<(TypeId, Vec<TypeParamId>), TypeId>,
    /// Cycle-tainted results with their dependency records, scoped to this run.
    tainted: FxHashMap<(TypeId, Vec<TypeParamId>), TaintedEntry>,
    /// Ids the current frame's subtree re-entered (raw-id cycle guard hits).
    frame_reentered: FxHashSet<TypeId>,
    /// Ids the current frame's subtree walked while they were not in progress.
    frame_visited: FxHashSet<TypeId>,
    /// Incremented by every raw-`TypeId` re-entry to taint all active ancestors.
    cycle_epoch: u64,
    /// Sorted map keys giving each param its prefilter bit; `None` disables the
    /// prefilter for this run (map too large — walking is always sound).
    param_keys: Option<Vec<TypeParamId>>,
    /// Per-node free-param bitmasks over `param_keys`, filled on demand by the
    /// Tarjan pass in [`compute_free_sets`]. Scoped to this run.
    free_sets: FxHashMap<TypeId, u64>,
    /// Exact binder-aware free sets after a positive approximate answer.
    exact_free_sets: FxHashMap<TypeId, u64>,
    /// Captured at construction so each run keeps one stable measurement owner.
    #[cfg(test)]
    measurement: Option<SubstitutionMeasureCollector>,
    #[cfg(test)]
    run_visit_measurement: Option<SubstitutionRunVisitMeasureCollector>,
}

impl<'a> Substitution<'a> {
    /// Build a substitution from a `TypeParamId → TypeId` map.
    pub fn new(map: &'a FxHashMap<TypeParamId, TypeId>) -> Self {
        #[cfg(test)]
        let measurement = capture_substitution_measurement();
        #[cfg(test)]
        let run_visit_measurement = capture_substitution_run_visit_measure();
        let param_keys = (map.len() <= PREFILTER_MAX_PARAMS).then(|| {
            let mut keys: Vec<TypeParamId> = map.keys().copied().collect();
            keys.sort_unstable();
            keys
        });
        Substitution {
            map,
            in_progress: FxHashSet::default(),
            blocked: FxHashSet::default(),
            completed: FxHashMap::default(),
            tainted: FxHashMap::default(),
            frame_reentered: FxHashSet::default(),
            frame_visited: FxHashSet::default(),
            cycle_epoch: 0,
            param_keys,
            free_sets: FxHashMap::default(),
            exact_free_sets: FxHashMap::default(),
            #[cfg(test)]
            measurement,
            #[cfg(test)]
            run_visit_measurement,
        }
    }

    /// Rewrite `ty`, replacing every type-parameter occurrence per the map and
    /// re-interning the result. Recurses through objects, functions, unions, and
    /// nested type parameters; an empty map (no parameters) leaves `ty` unchanged.
    pub fn apply(&mut self, interner: &mut Interner, ty: TypeId) -> TypeId {
        // Nothing to substitute (a non-generic instantiation, or a fully-resolved
        // subtree) — return as-is. This also keeps the common path allocation-free.
        if self.map.is_empty() {
            return ty;
        }

        // Param-relevant prefilter: when every map key free in `ty` is blocked
        // here (or none occurs at all), substitution is the identity — skip the
        // walk before any visit hook, memo entry, or interning.
        if self.effective_free_set_is_empty(interner, ty) {
            #[cfg(test)]
            if let Some(collector) = self.measurement.as_ref() {
                measure_substitution(collector, |measure| measure.prefilter_skips += 1);
            }
            return ty;
        }

        let result = 'apply: {
            #[cfg(test)]
            if let Some(collector) = self.measurement.as_ref() {
                measure_substitution_visit(collector, ty, &self.blocked);
            }
            #[cfg(test)]
            if let Some(collector) = self.run_visit_measurement.as_ref() {
                record_substitution_run_visit(collector, false);
            }
            // Raw-id re-entry must win over a completed result under any blocked
            // context; returning the original id is the existing cycle semantics.
            if self.in_progress.contains(&ty) {
                // Drift guard: the arms must never track a tag the visited filter drops.
                debug_assert!(tag_can_reenter(interner.store().tag(ty)));
                self.cycle_epoch += 1;
                // This branch never opens a frame, so this lands in the caller's
                // accumulator: the caller's result depends on `ty` being live.
                self.frame_reentered.insert(ty);
                #[cfg(test)]
                if let Some(collector) = self.measurement.as_ref() {
                    measure_substitution(collector, |measure| measure.cycle_reentries += 1);
                }
                break 'apply ty;
            }

            let key = (ty, self.canonical_blocked_context(ty));
            // A clean result observed no re-entry, so its closure is acyclic and a
            // reuse walk short-circuits here — no dependency record is needed.
            if let Some(&result) = self.completed.get(&key) {
                #[cfg(test)]
                if let Some(collector) = self.measurement.as_ref() {
                    measure_substitution(collector, |measure| measure.completed_memo_hits += 1);
                }
                #[cfg(test)]
                if let Some(collector) = self.run_visit_measurement.as_ref() {
                    let mut measure = collector.borrow_mut();
                    measure.completed_memo_hits = match measure.completed_memo_hits.checked_add(1) {
                        Some(value) => value,
                        None => {
                            measure.saturated = true;
                            u64::MAX
                        }
                    };
                }
                break 'apply result;
            }

            // Tainted-memo lookup: a stored cycle-tainted result is reusable iff
            // the current stack still cuts the graph identically — every id it
            // re-entered is still in progress, and nothing it walked plainly is.
            if let Some(entry) = self.tainted.get(&key) {
                if entry
                    .reentered
                    .iter()
                    .all(|id| self.in_progress.contains(id))
                    && self
                        .in_progress
                        .iter()
                        .all(|id| !entry.visited.contains(id))
                {
                    // The reuse is as cycle-dependent as the original walk: taint
                    // the consumer and inherit the record (plus the reused node
                    // itself), keeping the consumer out of `completed`.
                    self.cycle_epoch += 1;
                    let result = entry.result;
                    self.frame_reentered.extend(entry.reentered.iter().copied());
                    self.frame_visited.extend(entry.visited.iter().copied());
                    if tag_can_reenter(interner.store().tag(ty)) {
                        self.frame_visited.insert(ty);
                    }
                    #[cfg(test)]
                    if let Some(collector) = self.measurement.as_ref() {
                        measure_substitution(collector, |measure| measure.tainted_memo_hits += 1);
                    }
                    break 'apply result;
                }
                #[cfg(test)]
                if let Some(collector) = self.measurement.as_ref() {
                    measure_substitution(collector, |measure| {
                        measure.tainted_memo_stale_misses += 1;
                    });
                }
            }

            let start_cycle_epoch = self.cycle_epoch;
            let saved_reentered = std::mem::take(&mut self.frame_reentered);
            let saved_visited = std::mem::take(&mut self.frame_visited);
            let result = match interner.store().tag(ty) {
                // A type parameter: replace it with its argument if mapped; otherwise
                // (a parameter from an *outer* scope, not part of this substitution)
                // leave it untouched.
                TypeTag::TypeParam => {
                    let param_id = interner.store().type_param(ty).map(|p| p.id);
                    #[cfg(test)]
                    if let Some(collector) = self.measurement.as_ref() {
                        if param_id.is_some_and(|id| self.blocked.contains(&id)) {
                            measure_substitution(collector, |measure| {
                                measure.blocked_type_param_hits += 1;
                            });
                        }
                    }
                    match param_id
                        .filter(|id| !self.blocked.contains(id))
                        .and_then(|id| self.map.get(&id).copied())
                    {
                        Some(arg) => {
                            #[cfg(test)]
                            if let Some(collector) = self.measurement.as_ref() {
                                measure_substitution(collector, |measure| {
                                    measure.type_param_map_hits += 1;
                                });
                            }
                            arg
                        }
                        None => ty,
                    }
                }
                // Intrinsics and literals contain no type parameter — identity.
                TypeTag::Intrinsic | TypeTag::Literal => ty,
                TypeTag::Object => self.apply_object(interner, ty),
                TypeTag::Function => self.apply_function(interner, ty),
                TypeTag::Union => self.apply_union(interner, ty),
                TypeTag::Intersection => self.apply_intersection(interner, ty),
                TypeTag::Array => self.apply_array(interner, ty),
                TypeTag::Tuple => self.apply_tuple(interner, ty),
                TypeTag::Readonly => self.apply_readonly(interner, ty),
                TypeTag::Conditional => self.apply_conditional(interner, ty),
                TypeTag::Instantiation => self.apply_instantiation(interner, ty),
                TypeTag::ClassInstance => self.apply_class_instance(interner, ty),
                TypeTag::Mapped => self.apply_mapped(interner, ty),
                TypeTag::Template => self.apply_template(interner, ty),
                TypeTag::Keyof => self.apply_keyof(interner, ty),
                TypeTag::DeferredIndexedAccess => self.apply_deferred_indexed_access(interner, ty),
                // An `infer` binder (M25) / a mapped-value placeholder (M26) is a **bound**
                // node-scoped variable, never a free declaration parameter — the no-capture
                // rule (ADR-0002): substitution must leave it alone (the evaluator resolves
                // it, not this pass).
                TypeTag::Infer | TypeTag::MappedValue => ty,
            };

            let frame_reentered = std::mem::replace(&mut self.frame_reentered, saved_reentered);
            let frame_visited = std::mem::replace(&mut self.frame_visited, saved_visited);
            if self.cycle_epoch == start_cycle_epoch {
                // A clean subtree's record is discardable: reuse walks stop at the
                // `completed` hit above and never reach its closure again.
                debug_assert!(frame_reentered.is_empty());
                self.completed.insert(key, result);
                if tag_can_reenter(interner.store().tag(ty)) {
                    self.frame_visited.insert(ty);
                }
                #[cfg(test)]
                if let Some(collector) = self.measurement.as_ref() {
                    measure_substitution(collector, |measure| measure.completed_memo_entries += 1);
                }
            } else {
                // Internal-cycle fold: a frame that opened and closed inside this
                // subtree re-enters identically on any fresh walk, so `ty` leaves
                // its own record — reuse still rejects while `ty` is live via the
                // parent's `visited` (which receives `ty` below).
                let mut frame_reentered = frame_reentered;
                frame_reentered.remove(&ty);
                // The caller depends on everything this frame depended on.
                self.frame_reentered.extend(frame_reentered.iter().copied());
                self.frame_visited.extend(frame_visited.iter().copied());
                if tag_can_reenter(interner.store().tag(ty)) {
                    self.frame_visited.insert(ty);
                }
                self.tainted.insert(
                    key,
                    TaintedEntry {
                        result,
                        reentered: frame_reentered,
                        visited: frame_visited,
                    },
                );
                #[cfg(test)]
                if let Some(collector) = self.measurement.as_ref() {
                    measure_substitution(collector, |measure| {
                        measure.cycle_tainted_skips += 1;
                        measure.tainted_memo_entries += 1;
                    });
                }
            }

            result
        };
        result
    }

    /// Whether every map key free in `ty` is currently blocked (so `apply` may
    /// answer `ty` without walking). `false` whenever the prefilter is disabled.
    fn effective_free_set_is_empty(&mut self, interner: &Interner, ty: TypeId) -> bool {
        let Some(param_keys) = self.param_keys.as_deref() else {
            return false;
        };
        let bits = match self.free_sets.get(&ty) {
            Some(&bits) => bits,
            None => compute_free_sets(
                interner.store(),
                param_keys,
                &mut self.free_sets,
                #[cfg(test)]
                self.measurement.as_ref(),
                ty,
            ),
        };
        let blocked = self
            .blocked
            .iter()
            .fold(0, |mask, &id| mask | param_bit(param_keys, id));
        if bits & !blocked == 0 {
            return true;
        }
        let exact_bits = compute_exact_free_sets(
            interner.store(),
            param_keys,
            &mut self.exact_free_sets,
            #[cfg(test)]
            self.measurement.as_ref(),
            ty,
        );
        exact_bits & !blocked == 0
    }

    fn canonical_blocked_context(&self, ty: TypeId) -> Vec<TypeParamId> {
        if let (Some(param_keys), Some(&free_bits)) =
            (self.param_keys.as_deref(), self.exact_free_sets.get(&ty))
        {
            return param_keys
                .iter()
                .enumerate()
                .filter_map(|(index, &id)| {
                    (free_bits & (1u64 << index) != 0 && self.blocked.contains(&id)).then_some(id)
                })
                .collect();
        }

        // Large maps disable free-set analysis, so retain the full mapped context.
        let mut context: Vec<TypeParamId> = self
            .blocked
            .iter()
            .filter(|id| self.map.contains_key(id))
            .copied()
            .collect();
        context.sort_unstable();
        context
    }
}

/// Whether a check-parameter argument **distributes** a distributive conditional (M25):
/// a union (per-member evaluation), `never` (→ `never`), or the `boolean` intrinsic
/// (expands to `true | false` first). A single other type evaluates once — the plain
/// rewrite path.
fn distributes_over(interner: &Interner, arg: TypeId) -> bool {
    let wk = interner.well_known();
    interner.store().tag(arg) == TypeTag::Union || arg == wk.never || arg == wk.boolean
}

/// Convenience: instantiate `ty` with the given `TypeParamId → TypeId` map in one
/// call (builds a fresh [`Substitution`], applies it, drops it). Equal calls
/// produce equal interned ids.
pub fn substitute(
    interner: &mut Interner,
    ty: TypeId,
    map: &FxHashMap<TypeParamId, TypeId>,
) -> TypeId {
    match substitute_with_outcome(interner, ty, map) {
        SubstitutionOutcome::CycleClean(result) | SubstitutionOutcome::CycleTainted(result) => {
            result
        }
    }
}

/// Substitute while retaining whether any recursive re-entry affected the run.
pub(crate) fn substitute_with_outcome(
    interner: &mut Interner,
    ty: TypeId,
    map: &FxHashMap<TypeParamId, TypeId>,
) -> SubstitutionOutcome {
    let mut substitution = Substitution::new(map);
    let result = substitution.apply(interner, ty);
    if substitution.cycle_epoch == 0 {
        SubstitutionOutcome::CycleClean(result)
    } else {
        SubstitutionOutcome::CycleTainted(result)
    }
}

/// Instantiate a generic function's own binders for one call candidate.
///
/// Unlike [`substitute`], this consumes the outer function's binder list while
/// preserving binders nested inside parameter or return types.
pub fn instantiate_function(
    interner: &mut Interner,
    ty: TypeId,
    map: &FxHashMap<TypeParamId, TypeId>,
) -> TypeId {
    let Some(function) = interner.store().function_type(ty) else {
        return ty;
    };
    let receiver = function.receiver;
    let params = function.params.clone();
    let ret = function.ret;
    let mut substitution = Substitution::new(map);
    let params = params
        .into_iter()
        .map(|mut param| {
            param.ty = substitution.apply(interner, param.ty);
            param
        })
        .collect();
    let ret = substitution.apply(interner, ret);
    let receiver = receiver.map(|receiver| substitution.apply(interner, receiver));
    interner.intern_function(FunctionType {
        type_params: Vec::new(),
        receiver,
        params,
        ret,
    })
}

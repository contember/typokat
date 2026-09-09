//! Low-level flow-node machinery and the backward resolution walk
//! (extracted from flowgraph.rs).

use crate::binder::symbol::SymbolId;
use crate::check::flow::{assignment_reduce_query, narrow_query, FlowNode, FlowNodeId};
use crate::check::query::SemanticQueryAttempt;
use crate::class_semantics::DemandOutcome;
use crate::types::store::TypeId;
use rustc_hash::FxHashMap;

use super::super::context::*;
use super::super::narrowing::GuardFact;
use super::*;

type PendingFlowWrites = FxHashMap<(FlowNodeId, SymbolId), FlowValueState>;

#[cfg(test)]
#[path = "../tests/assignment_flow.rs"]
mod tests;

macro_rules! ready_flow {
    ($outcome:expr) => {
        match $outcome {
            SemanticQueryAttempt::Complete(DemandOutcome::Ready(value)) => value,
            SemanticQueryAttempt::Complete(DemandOutcome::Exhausted(reason)) => {
                return SemanticQueryAttempt::Complete(DemandOutcome::Exhausted(reason));
            }
            SemanticQueryAttempt::NeedsNativeIdentities(demands) => {
                return SemanticQueryAttempt::NeedsNativeIdentities(demands);
            }
        }
    };
}

impl<'a, 'ast, Ticket: Copy + PartialEq> Pass<'a, 'ast, Ticket> {
    /// Append a flow node, returning its id.
    pub(super) fn new_flow(&mut self, node: FlowNode) -> FlowNodeId {
        let id = FlowNodeId(self.flow_nodes.len() as u32);
        self.flow_nodes.push(node);
        id
    }

    /// A condition node for one branch of `fact` from `antecedent`, with the branch
    /// polarity already folded (`then_branch` applies the fact as written, else its
    /// negation). An unrecognized guard (`None`) narrows nothing — both branches
    /// share the antecedent.
    pub(super) fn flow_condition(
        &mut self,
        antecedent: FlowNodeId,
        fact: &Option<GuardFact>,
        then_branch: bool,
    ) -> FlowNodeId {
        if antecedent == FlowNodeId::UNREACHABLE {
            return FlowNodeId::UNREACHABLE;
        }
        match fact {
            Some(fact) => {
                let positive = if then_branch {
                    fact.then_positive
                } else {
                    !fact.then_positive
                };
                self.new_flow(FlowNode::Condition {
                    symbol: fact.symbol,
                    op: fact.op.clone(),
                    positive,
                    antecedent,
                })
            }
            None => antecedent,
        }
    }

    /// Join branch antecedents, excluding the unreachable ones. Empty → unreachable;
    /// one → that node (no label); more → a fresh branch label.
    pub(super) fn flow_join(&mut self, antecedents: Vec<FlowNodeId>) -> FlowNodeId {
        let mut reachable: Vec<FlowNodeId> = antecedents
            .into_iter()
            .filter(|&a| a != FlowNodeId::UNREACHABLE)
            .collect();
        reachable.sort_by_key(|id| id.0);
        reachable.dedup();
        match reachable.as_slice() {
            [] => FlowNodeId::UNREACHABLE,
            [only] => *only,
            _ => self.new_flow(FlowNode::BranchLabel {
                antecedents: reachable,
            }),
        }
    }

    /// Append `edge` as a back edge of a loop `label` (unless the edge is
    /// unreachable, e.g. a body ending in `return`/`break`).
    pub(super) fn add_back_edge(&mut self, label: FlowNodeId, edge: FlowNodeId) {
        if edge == FlowNodeId::UNREACHABLE {
            return;
        }
        if let Some(FlowNode::LoopLabel { back, .. }) = self.flow_nodes.get_mut(label.0 as usize) {
            back.push(edge);
        }
    }

    // =======================================================================
    // Resolution — the backward walk.
    // =======================================================================

    /// A value symbol's complete declared type, used at `START` and resets.
    fn declared_flow_value(&self, symbol: SymbolId) -> FlowValueState {
        debug_assert!(
            !self.function_groups.requires_demand_intercept(symbol),
            "unpublished function groups must not enter durable flow resolution"
        );
        let Some(storage) = self.effective_value_storage_for_symbol(symbol) else {
            return FlowValueState {
                ty: self.interner.well_known().error,
            };
        };
        FlowValueState {
            ty: self
                .decl_type(storage)
                .unwrap_or_else(|| self.interner.well_known().error),
        }
    }

    /// The already-known type at a flow node for `symbol`, if any: the `START`/
    /// `UNREACHABLE` sentinels resolve to the declared type; a provisional loop seed
    /// or a durable memo entry short-circuits. `None` means "must walk the node".
    fn flow_lookup_cached(
        &self,
        node: FlowNodeId,
        symbol: SymbolId,
        pending: &PendingFlowWrites,
    ) -> Option<FlowValueState> {
        if node == FlowNodeId::START || node == FlowNodeId::UNREACHABLE {
            return Some(self.declared_flow_value(symbol));
        }
        if let Some(&state) = self.flow_provisional.get(&(node, symbol)) {
            return Some(state);
        }
        pending
            .get(&(node, symbol))
            .or_else(|| self.flow_memo.get(&(node, symbol)))
            .copied()
    }

    /// Resolve `symbol`'s narrowed type at flow node `flow` — the memoized backward
    /// walk (architecture §5). Iterative (explicit stack) for the non-loop graph so a
    /// deep nest cannot overflow the host stack; a loop label recurses through
    /// [`resolve_loop_label`] (bounded by loop-nesting depth).
    pub(in crate::check::checker) fn resolve_narrowed_type(
        &mut self,
        flow: FlowNodeId,
        symbol: SymbolId,
    ) -> DemandOutcome<TypeId> {
        match self
            .retry_native_identity_demands(|pass| pass.resolve_narrowed_type_attempt(flow, symbol))
        {
            Ok(outcome) => outcome,
            Err(()) => DemandOutcome::Exhausted(
                crate::class_semantics::Exhaustion::EvaluationInvalidNode {
                    ty: self.declared_flow_value(symbol).ty,
                },
            ),
        }
    }

    fn resolve_narrowed_type_attempt(
        &mut self,
        flow: FlowNodeId,
        symbol: SymbolId,
    ) -> SemanticQueryAttempt<DemandOutcome<TypeId>> {
        self.semantic_queries.savepoint();
        let mut pending = PendingFlowWrites::default();
        let outcome = self.resolve_narrowed_flow_value(flow, symbol, &mut pending);
        match outcome {
            SemanticQueryAttempt::Complete(DemandOutcome::Ready(value)) => {
                self.semantic_queries.commit();
                self.flow_memo.extend(pending);
                SemanticQueryAttempt::Complete(DemandOutcome::Ready(value.ty))
            }
            SemanticQueryAttempt::Complete(DemandOutcome::Exhausted(reason)) => {
                self.semantic_queries.rollback();
                SemanticQueryAttempt::Complete(DemandOutcome::Exhausted(reason))
            }
            SemanticQueryAttempt::NeedsNativeIdentities(demands) => {
                self.semantic_queries.rollback();
                SemanticQueryAttempt::NeedsNativeIdentities(demands)
            }
        }
    }

    fn resolve_narrowed_flow_value(
        &mut self,
        flow: FlowNodeId,
        symbol: SymbolId,
        pending: &mut PendingFlowWrites,
    ) -> SemanticQueryAttempt<DemandOutcome<FlowValueState>> {
        if let Some(state) = self.flow_lookup_cached(flow, symbol, pending) {
            return SemanticQueryAttempt::Complete(DemandOutcome::Ready(state));
        }
        // Per-call scratch: within one walk every node is computed once (bounds the
        // work and keeps the explicit stack from re-descending shared antecedents).
        let mut scratch: FxHashMap<FlowNodeId, FlowValueState> = FxHashMap::default();
        let mut stack: Vec<FlowNodeId> = vec![flow];
        while let Some(&node) = stack.last() {
            if scratch.contains_key(&node) {
                stack.pop();
                continue;
            }
            if let Some(state) = self.flow_lookup_cached(node, symbol, pending) {
                scratch.insert(node, state);
                stack.pop();
                continue;
            }
            match self.flow_step(node) {
                FlowStep::Terminal(t) => {
                    self.flow_set(
                        &mut scratch,
                        pending,
                        node,
                        symbol,
                        FlowValueState { ty: t },
                    );
                    stack.pop();
                }
                FlowStep::Assignment {
                    target,
                    assigned,
                    assigned_source,
                    antecedent,
                } => {
                    if target == symbol {
                        let source = match (assigned, assigned_source) {
                            (Some(ty), _) => Some(ty),
                            (None, Some(source)) => Some(
                                ready_flow!(
                                    self.resolve_narrowed_flow_value(antecedent, source, pending)
                                )
                                .ty,
                            ),
                            (None, None) => None,
                        };
                        let declared = self.declared_flow_value(symbol).ty;
                        let ty = match source {
                            Some(source) => ready_flow!(assignment_reduce_query(
                                self.interner,
                                self.type_environment.published(),
                                &mut self.semantic_queries,
                                &mut self.next_type_param,
                                declared,
                                source,
                            )),
                            None => declared,
                        };
                        self.flow_set(&mut scratch, pending, node, symbol, FlowValueState { ty });
                        stack.pop();
                    } else if let Some(base) =
                        self.scratch_or_cached(&scratch, pending, antecedent, symbol)
                    {
                        self.flow_set(&mut scratch, pending, node, symbol, base);
                        stack.pop();
                    } else {
                        stack.push(antecedent);
                    }
                }
                FlowStep::Condition {
                    guard_symbol,
                    op,
                    positive,
                    antecedent,
                } => {
                    if let Some(base) =
                        self.scratch_or_cached(&scratch, pending, antecedent, symbol)
                    {
                        let t = if guard_symbol == symbol {
                            ready_flow!(narrow_query(
                                self.interner,
                                self.type_environment.published(),
                                &mut self.semantic_queries,
                                &mut self.next_type_param,
                                base.ty,
                                &op,
                                positive,
                            ))
                        } else {
                            base.ty
                        };
                        self.flow_set(
                            &mut scratch,
                            pending,
                            node,
                            symbol,
                            FlowValueState { ty: t },
                        );
                        stack.pop();
                    } else {
                        stack.push(antecedent);
                    }
                }
                FlowStep::Branch { antecedents } => {
                    let missing = antecedents.iter().copied().find(|&a| {
                        self.scratch_or_cached(&scratch, pending, a, symbol)
                            .is_none()
                    });
                    if let Some(a) = missing {
                        stack.push(a);
                    } else {
                        let parts: Vec<FlowValueState> = antecedents
                            .iter()
                            .map(|&a| {
                                self.scratch_or_cached(&scratch, pending, a, symbol)
                                    .unwrap_or_else(|| self.declared_flow_value(symbol))
                            })
                            .collect();
                        let ty = self
                            .interner
                            .union(parts.iter().map(|part| part.ty).collect());
                        self.flow_set(&mut scratch, pending, node, symbol, FlowValueState { ty });
                        stack.pop();
                    }
                }
                FlowStep::Loop => {
                    let state = ready_flow!(self.resolve_loop_label(node, symbol, pending));
                    // Only the discharged label enters pending writes.
                    scratch.insert(node, state);
                    stack.pop();
                }
            }
        }
        SemanticQueryAttempt::Complete(DemandOutcome::Ready(
            scratch
                .get(&flow)
                .copied()
                .unwrap_or_else(|| self.declared_flow_value(symbol)),
        ))
    }

    /// Resolve a `while` label with a single-unroll fixpoint. The provisional seed
    /// is never promoted to the durable memo (invariants §1), and durable writes are
    /// suppressed while any loop fixpoint is in progress.
    fn resolve_loop_label(
        &mut self,
        label: FlowNodeId,
        symbol: SymbolId,
        pending: &mut PendingFlowWrites,
    ) -> SemanticQueryAttempt<DemandOutcome<FlowValueState>> {
        if let Some(&state) = self.flow_provisional.get(&(label, symbol)) {
            return SemanticQueryAttempt::Complete(DemandOutcome::Ready(state));
        }
        if self.flow_loop_depth == 0 {
            if let Some(state) = self.flow_lookup_cached(label, symbol, pending) {
                return SemanticQueryAttempt::Complete(DemandOutcome::Ready(state));
            }
        }
        let (pre, back) = match self.flow_nodes.get(label.0 as usize) {
            Some(FlowNode::LoopLabel { pre, back }) => (pre.clone(), back.clone()),
            _ => {
                return SemanticQueryAttempt::Complete(DemandOutcome::Ready(
                    self.declared_flow_value(symbol),
                ))
            }
        };

        self.flow_loop_depth += 1;
        // Guard re-entry during pre/seed resolution with the declared type (sound;
        // pre edges do not normally reach the label, but a malformed graph must still
        // terminate).
        let declared = self.declared_flow_value(symbol);
        self.flow_provisional.insert((label, symbol), declared);

        self.semantic_queries.savepoint();
        let outcome = (|| {
            let mut pre_parts: Vec<FlowValueState> = Vec::with_capacity(pre.len());
            for antecedent in &pre {
                pre_parts.push(ready_flow!(self.resolve_narrowed_flow_value(
                    *antecedent,
                    symbol,
                    pending
                )));
            }
            let seed = FlowValueState {
                ty: self
                    .interner
                    .union(pre_parts.iter().map(|part| part.ty).collect()),
            };
            // The seed is now the value a re-entrant back-edge walk sees.
            self.flow_provisional.insert((label, symbol), seed);

            let mut parts: Vec<FlowValueState> = vec![seed];
            for antecedent in &back {
                parts.push(ready_flow!(self.resolve_narrowed_flow_value(
                    *antecedent,
                    symbol,
                    pending
                )));
            }
            let result = FlowValueState {
                ty: self
                    .interner
                    .union(parts.iter().map(|part| part.ty).collect()),
            };
            SemanticQueryAttempt::Complete(DemandOutcome::Ready(result))
        })();
        self.semantic_queries.rollback();
        self.flow_provisional.remove(&(label, symbol));
        self.flow_loop_depth -= 1;
        if let SemanticQueryAttempt::Complete(DemandOutcome::Ready(result)) = &outcome {
            if self.flow_loop_depth == 0 {
                pending.insert((label, symbol), *result);
            }
        }
        outcome
    }

    /// Look a node's type up in the per-call scratch, then the cross-call caches.
    fn scratch_or_cached(
        &self,
        scratch: &FxHashMap<FlowNodeId, FlowValueState>,
        pending: &PendingFlowWrites,
        node: FlowNodeId,
        symbol: SymbolId,
    ) -> Option<FlowValueState> {
        scratch
            .get(&node)
            .copied()
            .or_else(|| self.flow_lookup_cached(node, symbol, pending))
    }

    /// Only discharged values may enter the outer walk's pending writes.
    fn flow_set(
        &mut self,
        scratch: &mut FxHashMap<FlowNodeId, FlowValueState>,
        pending: &mut PendingFlowWrites,
        node: FlowNodeId,
        symbol: SymbolId,
        state: FlowValueState,
    ) {
        scratch.insert(node, state);
        if self.flow_loop_depth == 0 {
            pending.insert((node, symbol), state);
        }
    }

    /// Snapshot a flow node's resolution-relevant data (cloning the small owned bits
    /// so the `&mut Interner` narrow/union calls do not conflict with the arena
    /// borrow). `START`/`UNREACHABLE` are handled by the caller before this, so the
    /// defensive fallthrough (`Terminal(error)`) is unreachable.
    fn flow_step(&self, node: FlowNodeId) -> FlowStep {
        match self.flow_nodes.get(node.0 as usize) {
            Some(FlowNode::Assignment {
                symbol,
                assigned,
                assigned_source,
                antecedent,
            }) => FlowStep::Assignment {
                target: *symbol,
                assigned: *assigned,
                assigned_source: *assigned_source,
                antecedent: *antecedent,
            },
            Some(FlowNode::Condition {
                symbol,
                op,
                positive,
                antecedent,
            }) => FlowStep::Condition {
                guard_symbol: *symbol,
                op: op.clone(),
                positive: *positive,
                antecedent: *antecedent,
            },
            Some(FlowNode::BranchLabel { antecedents }) => FlowStep::Branch {
                antecedents: antecedents.clone(),
            },
            Some(FlowNode::LoopLabel { .. }) => FlowStep::Loop,
            _ => FlowStep::Terminal(self.interner.well_known().error),
        }
    }
}

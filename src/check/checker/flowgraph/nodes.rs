//! Low-level flow-node machinery and the backward resolution walk
//! (extracted from flowgraph.rs).

use crate::binder::symbol::SymbolId;
use crate::check::flow::{narrow_query, FlowNode, FlowNodeId};
use crate::class_semantics::DemandOutcome;
use crate::types::store::TypeId;
use rustc_hash::FxHashMap;

use super::super::context::*;
use super::super::narrowing::GuardFact;
use super::*;

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

    /// A value symbol's **declared** type — the backward walk's base case (at
    /// `START`) and the reset target for a `None` assignment. Mirrors the pre-M23
    /// `resolve_identifier_type` fallback.
    pub(in crate::check::checker) fn declared_type(&self, symbol: SymbolId) -> TypeId {
        debug_assert!(
            !self.function_groups.requires_demand_intercept(symbol),
            "unpublished function groups must not enter durable flow resolution"
        );
        self.binder
            .symbols
            .get(symbol)
            .and_then(|s| s.value)
            .and_then(|decl_id| self.decl_types.get(decl_id))
            .unwrap_or_else(|| self.interner.well_known().error)
    }

    /// The already-known type at a flow node for `symbol`, if any: the `START`/
    /// `UNREACHABLE` sentinels resolve to the declared type; a provisional loop seed
    /// or a durable memo entry short-circuits. `None` means "must walk the node".
    fn flow_lookup_cached(&self, node: FlowNodeId, symbol: SymbolId) -> Option<TypeId> {
        if node == FlowNodeId::START || node == FlowNodeId::UNREACHABLE {
            return Some(self.declared_type(symbol));
        }
        if let Some(&t) = self.flow_provisional.get(&(node, symbol)) {
            return Some(t);
        }
        self.flow_memo.get(&(node, symbol)).copied()
    }

    /// Resolve `symbol`'s narrowed type at flow node `flow` — the memoized backward
    /// walk (architecture §5). Iterative (explicit stack) for the non-loop graph so a
    /// deep nest cannot overflow the host stack; a loop label recurses through
    /// [`resolve_loop_label`] (bounded by loop-nesting depth).
    pub(in crate::check::checker) fn resolve_narrowed_type(
        &mut self,
        flow: FlowNodeId,
        symbol: SymbolId,
    ) -> TypeId {
        if let Some(t) = self.flow_lookup_cached(flow, symbol) {
            return t;
        }
        // Per-call scratch: within one walk every node is computed once (bounds the
        // work and keeps the explicit stack from re-descending shared antecedents).
        let mut scratch: FxHashMap<FlowNodeId, TypeId> = FxHashMap::default();
        let mut stack: Vec<FlowNodeId> = vec![flow];
        while let Some(&node) = stack.last() {
            if scratch.contains_key(&node) {
                stack.pop();
                continue;
            }
            if let Some(t) = self.flow_lookup_cached(node, symbol) {
                scratch.insert(node, t);
                stack.pop();
                continue;
            }
            match self.flow_step(node) {
                FlowStep::Terminal(t) => {
                    self.flow_set(&mut scratch, node, symbol, t);
                    stack.pop();
                }
                FlowStep::Assignment {
                    target,
                    assigned,
                    antecedent,
                } => {
                    if target == symbol {
                        let t = assigned.unwrap_or_else(|| self.declared_type(symbol));
                        self.flow_set(&mut scratch, node, symbol, t);
                        stack.pop();
                    } else if let Some(base) = self.scratch_or_cached(&scratch, antecedent, symbol)
                    {
                        self.flow_set(&mut scratch, node, symbol, base);
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
                    if let Some(base) = self.scratch_or_cached(&scratch, antecedent, symbol) {
                        let t = if guard_symbol == symbol {
                            match narrow_query(
                                self.interner,
                                self.type_environment.published().classes(),
                                &mut self.semantic_queries,
                                &mut self.next_type_param,
                                base,
                                &op,
                                positive,
                            ) {
                                DemandOutcome::Ready(narrowed) => narrowed,
                                DemandOutcome::Exhausted(_) => base,
                            }
                        } else {
                            base
                        };
                        self.flow_set(&mut scratch, node, symbol, t);
                        stack.pop();
                    } else {
                        stack.push(antecedent);
                    }
                }
                FlowStep::Branch { antecedents } => {
                    let missing = antecedents
                        .iter()
                        .copied()
                        .find(|&a| self.scratch_or_cached(&scratch, a, symbol).is_none());
                    if let Some(a) = missing {
                        stack.push(a);
                    } else {
                        let parts: Vec<TypeId> = antecedents
                            .iter()
                            .map(|&a| {
                                self.scratch_or_cached(&scratch, a, symbol)
                                    .unwrap_or_else(|| self.declared_type(symbol))
                            })
                            .collect();
                        let t = self.interner.union(parts);
                        self.flow_set(&mut scratch, node, symbol, t);
                        stack.pop();
                    }
                }
                FlowStep::Loop => {
                    let t = self.resolve_loop_label(node, symbol);
                    // `resolve_loop_label` handles the durable memo itself.
                    scratch.insert(node, t);
                    stack.pop();
                }
            }
        }
        scratch
            .get(&flow)
            .copied()
            .unwrap_or_else(|| self.declared_type(symbol))
    }

    /// Resolve a `while` label with a single-unroll fixpoint. The provisional seed
    /// is never promoted to the durable memo (invariants §1), and durable writes are
    /// suppressed while any loop fixpoint is in progress.
    fn resolve_loop_label(&mut self, label: FlowNodeId, symbol: SymbolId) -> TypeId {
        if let Some(&t) = self.flow_provisional.get(&(label, symbol)) {
            return t;
        }
        if self.flow_loop_depth == 0 {
            if let Some(&t) = self.flow_memo.get(&(label, symbol)) {
                return t;
            }
        }
        let (pre, back) = match self.flow_nodes.get(label.0 as usize) {
            Some(FlowNode::LoopLabel { pre, back }) => (pre.clone(), back.clone()),
            _ => return self.declared_type(symbol),
        };

        self.flow_loop_depth += 1;
        // Guard re-entry during pre/seed resolution with the declared type (sound;
        // pre edges do not normally reach the label, but a malformed graph must still
        // terminate).
        let declared = self.declared_type(symbol);
        self.flow_provisional.insert((label, symbol), declared);

        let mut pre_parts: Vec<TypeId> = Vec::with_capacity(pre.len());
        for antecedent in &pre {
            let t = self.resolve_narrowed_type(*antecedent, symbol);
            pre_parts.push(t);
        }
        let seed = self.interner.union(pre_parts);
        // The seed is now the value a re-entrant back-edge walk sees.
        self.flow_provisional.insert((label, symbol), seed);

        let mut parts: Vec<TypeId> = vec![seed];
        for antecedent in &back {
            let t = self.resolve_narrowed_type(*antecedent, symbol);
            parts.push(t);
        }
        self.flow_provisional.remove(&(label, symbol));
        self.flow_loop_depth -= 1;

        let result = self.interner.union(parts);
        if self.flow_loop_depth == 0 {
            self.flow_memo.insert((label, symbol), result);
        }
        result
    }

    /// Look a node's type up in the per-call scratch, then the cross-call caches.
    fn scratch_or_cached(
        &self,
        scratch: &FxHashMap<FlowNodeId, TypeId>,
        node: FlowNodeId,
        symbol: SymbolId,
    ) -> Option<TypeId> {
        scratch
            .get(&node)
            .copied()
            .or_else(|| self.flow_lookup_cached(node, symbol))
    }

    /// Record a resolved node type in the per-call scratch and — unless a loop
    /// fixpoint is in progress (its value could depend on a provisional seed) — the
    /// durable memo.
    fn flow_set(
        &mut self,
        scratch: &mut FxHashMap<FlowNodeId, TypeId>,
        node: FlowNodeId,
        symbol: SymbolId,
        ty: TypeId,
    ) {
        scratch.insert(node, ty);
        if self.flow_loop_depth == 0 {
            self.flow_memo.insert((node, symbol), ty);
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
                antecedent,
            }) => FlowStep::Assignment {
                target: *symbol,
                assigned: *assigned,
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

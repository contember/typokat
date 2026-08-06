//! Shared checker-pass data types (architecture §5).
//! The `checker` submodules all operate on [`Pass`], so this module owns the
//! obligation, declaration, class, and flow bookkeeping visible within the tree.

use crate::binder::declaration::{DeclId, TypeGroupId, ValueStorageId};
use crate::binder::namespace::SourceUnitKey;
use crate::binder::scope::ScopeId;
use crate::binder::symbol::SymbolId;
use crate::binder::Binder;
use crate::check::flow::{FlowNode, FlowNodeId};
use crate::check::query::SemanticQueryState;
use crate::source::{LibraryFileOrdinal, SourceUnit};
use crate::span::Span;
use crate::types::layered::{LayeredMap, LayeredSet, LayeredVec};
use crate::types::repr::{ClassId, PropertyType, TypeParamId, Visibility};
use crate::types::store::TypeId;
use crate::types::Interner;
use oxc_ast::ast::{Class, TSInterfaceHeritage, TSType, TSTypeParameterDeclaration};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::{Deref, DerefMut, Index, IndexMut};
use std::sync::Arc;

use super::calls::{
    ArgumentWalkKey, ContextualWalkKey, MemoizedArgumentWalk, MemoizedContextualWalk,
};
use super::classes::body::{BodyClassView, BodyMemberEnvironment};
use super::classes::construction::DraftClassTypeParameter;
use super::classes::publication::StagedClassValidation;
use super::classes::retained::RetainedClassCallable;
use super::events::{CandidateEffects, UserRecordTicket};
use super::function_groups::FunctionGroupRegistry;
use super::lexical_events::{CallableTickets, LexicalReservations};
use super::library_identities::{LibrarySemanticIdentities, NativeArrayGroups};
use super::namespace_values::NamespaceValueRegistry;
use super::replay_index::ReplayDependencyTrace;
use super::replay_index::ReplayOwner;
use super::reporting_record::CheckerRecord;
use super::type_groups::{
    InterfaceTypedAlternative, PublishedTypeParameterDefault, TypeEnvironmentState,
    TypeGroupConstruction,
};

/// Authenticated value-space roots that may authorize syntax-directed lowering.
///
/// These ids come only from the retained library root projection. A same-spelled
/// user binding therefore cannot acquire library authority.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::check::checker) struct CertifiedLibraryValues {
    pub(in crate::check::checker) symbol: Option<ValueStorageId>,
}

/// Which diagnostic an assignability obligation produces on failure. The
/// structural verdict is the same relation query; only the code/message mapping
/// differs (mvp-plan §6 "code mapping").
#[derive(Copy, Clone, PartialEq, Eq)]
pub(in crate::check::checker) enum ObligationKind {
    /// Annotation-vs-initializer, reassignment, or `return`-vs-declared-return.
    /// Maps a missing-property reason to `TK2741`, everything else to `TK2322`.
    Assignment,
    /// A call argument vs its parameter. Context-free argument failures map to
    /// `TK2345`; contextually typed fresh object/tuple literals use assignment-style
    /// diagnostics for their member/element mismatch parity with tsc.
    Argument,
    /// A contextually typed fresh literal in a call argument. Most structural
    /// mismatches stay assignment-style, but missing required properties and
    /// tuple length mismatches remain call-argument failures.
    FreshArgument,
}

/// One assignability obligation: `src` must be assignable to `tgt`, with the
/// resulting diagnostic's primary span at `src_span` and its code determined by
/// `kind`.
pub(in crate::check::checker) struct AssignObligation {
    pub(in crate::check::checker) src: TypeId,
    pub(in crate::check::checker) tgt: TypeId,
    pub(in crate::check::checker) src_span: Span,
    pub(in crate::check::checker) source_member_spans: Vec<(AssignSourceMember, Span)>,
    pub(in crate::check::checker) kind: ObligationKind,
}

#[derive(Clone, PartialEq, Eq)]
pub(in crate::check::checker) enum AssignSourceMember {
    Property(String),
    Element(usize),
}

/// The assertion spelling determines the stable incomplete-surface identity.
#[derive(Copy, Clone, PartialEq, Eq)]
pub(in crate::check::checker) enum AssertionSyntax {
    As,
    Angle,
}

impl AssertionSyntax {
    pub(in crate::check::checker) const fn incomplete_id(self) -> &'static str {
        match self {
            Self::As => "expr-infer/as-assertion/compatibility",
            Self::Angle => "expr-infer/type-assertion/compatibility",
        }
    }
}

/// One assertion whose source/target overlap proof waits for semantic replay.
pub(in crate::check::checker) struct AssertionCompatibilityObligation {
    pub(in crate::check::checker) source: TypeId,
    pub(in crate::check::checker) asserted: TypeId,
    pub(in crate::check::checker) span: Span,
    pub(in crate::check::checker) syntax: AssertionSyntax,
}

/// Relation work retains one scheduling order within its lexical effect owner.
pub(in crate::check::checker) enum DeferredRelationObligation {
    Assign(AssignObligation),
    AssertionCompatibility(AssertionCompatibilityObligation),
}

/// One generic application whose constraint relation is intentionally delayed
/// until the complete class and type-group registries are immutable.
pub(in crate::check::checker) struct ConstraintCheckObligation {
    pub(in crate::check::checker) checks: Vec<(Option<TypeId>, TypeId, Span)>,
    pub(in crate::check::checker) substitutions: FxHashMap<TypeParamId, TypeId>,
}

pub(in crate::check::checker) enum InterfaceRelationKind {
    NumberIndex,
    PropertyStringIndex { name: String },
    Heritage { left: String, right: String },
    MergedProperty { name: String },
    HeaderMetadata { name: String },
    HeritageMember { derived: String, base: String },
    HeritageIndex { derived: String, base: String },
}

pub(in crate::check::checker) struct InterfaceRelationObligation {
    pub(in crate::check::checker) source: TypeId,
    pub(in crate::check::checker) target: TypeId,
    pub(in crate::check::checker) span: Span,
    pub(in crate::check::checker) kind: InterfaceRelationKind,
    pub(in crate::check::checker) report: InterfaceRelationReport,
}

#[derive(Copy, Clone)]
pub(in crate::check::checker) enum InterfaceRelationReport {
    Always,
    FirstFailedHeritagePair(u32),
    FirstFailedHeaderGroup(crate::binder::declaration::TypeGroupId),
}

/// One class-member override-compatibility check (`TK2416`).
/// Collected at fill time, decided in phase 2, and kept separate from
/// [`AssignObligation`] because method overrides use tsc's bivariant-param /
/// covariant-return rule rather than one whole-type relation query.
pub(in crate::check::checker) struct OverrideCheck {
    pub(in crate::check::checker) own_ty: TypeId,
    pub(in crate::check::checker) base_ty: TypeId,
    pub(in crate::check::checker) name: String,
    pub(in crate::check::checker) derived: String,
    pub(in crate::check::checker) base: String,
    pub(in crate::check::checker) span: Span,
    /// Whether the base member used method syntax.
    /// tsc keys the override variance rule on the base member kind; the derived
    /// member's kind only affects diagnostic positioning.
    pub(in crate::check::checker) base_is_method: bool,
}

/// One lexical owner's ordered records and semantic work. Nested speculative
/// children remain local until the selected child merges into its parent.
pub(in crate::check::checker) struct CheckerRecordBatch<Ticket: Copy> {
    owner: Ticket,
    records: Vec<CheckerRecord>,
}

impl<Ticket: Copy> CheckerRecordBatch<Ticket> {
    fn new(owner: Ticket) -> Self {
        Self {
            owner,
            records: Vec::new(),
        }
    }

    fn from_parts(owner: Ticket, records: Vec<CheckerRecord>) -> Self {
        Self { owner, records }
    }

    pub(in crate::check::checker) fn diagnostic(
        &mut self,
        diagnostic: crate::diagnostics::Diagnostic,
    ) {
        self.records.push(CheckerRecord::Diagnostic(diagnostic));
    }

    pub(in crate::check::checker) fn incomplete(
        &mut self,
        incomplete: crate::diagnostics::IncompleteSurface,
    ) {
        self.records.push(CheckerRecord::Incomplete(incomplete));
    }

    pub(in crate::check::checker) fn record(&mut self, record: CheckerRecord) {
        self.records.push(record);
    }

    pub(in crate::check::checker) const fn owner(&self) -> Ticket {
        self.owner
    }

    pub(in crate::check::checker) fn into_parts(self) -> (Ticket, Vec<CheckerRecord>) {
        (self.owner, self.records)
    }

    pub(in crate::check::checker) fn discard(self) {}

    #[cfg(test)]
    pub(in crate::check::checker) fn len(&self) -> usize {
        self.records.len()
    }

    #[cfg(test)]
    fn records(&self) -> &[CheckerRecord] {
        &self.records
    }
}

impl<Ticket: Copy + PartialEq> CheckerRecordBatch<Ticket> {
    fn merge(&mut self, child: Self) {
        assert!(
            self.owner == child.owner,
            "nested effects must share one owner"
        );
        self.records.extend(child.records);
    }
}

/// One call/`new` argument's raw walk, as the in-flight call frame sees it.
///
/// Exactly one walk of a re-walkable argument may report. The raw walk runs first, so
/// its records are parked here until the committed contextual walk says whether it
/// superseded them (backlog `92`); whatever is still parked when the frame closes
/// commits.
pub(in crate::check::checker) enum ProvisionalArgumentWalk<Ticket: Copy = UserRecordTicket> {
    /// Nothing is parked: either the shape admits no contextual re-walk and the raw
    /// walk already reported in place, or the committed walk superseded it.
    Settled,
    /// The raw walk's records, parked until the committed walk reports.
    Held(CheckerEffects<Ticket>),
    /// The raw walk was served from [`Pass::raw_argument_walk_memo`], so no records
    /// exist for this argument yet (backlog `95`). If the committed contextual walk
    /// supersedes the argument this is the whole saving; if it does **not**, the walk
    /// is re-run before the frame closes and parks its records here, so the raw walk
    /// still reports wherever it is the only walk. The index is the argument's
    /// position in the call's own argument list, which is what makes that re-walk
    /// possible.
    Memoized { argument_index: usize },
}

pub(in crate::check::checker) struct CheckerEffects<Ticket: Copy = UserRecordTicket> {
    pub(in crate::check::checker) records: CheckerRecordBatch<Ticket>,
    activated: bool,
    pub(in crate::check::checker) obligations: Vec<DeferredRelationObligation>,
    pub(in crate::check::checker) constraint_checks: Vec<ConstraintCheckObligation>,
    pub(in crate::check::checker) interface_relations: Vec<InterfaceRelationObligation>,
    pub(in crate::check::checker) override_checks: Vec<OverrideCheck>,
    replay_owners: Option<Box<DeferredReplayOwners>>,
    pub(in crate::check::checker) nested: Vec<CheckerEffects<Ticket>>,
}

#[derive(Default)]
struct DeferredReplayOwners {
    obligations: Vec<Option<ReplayOwner>>,
    constraint_checks: Vec<Option<ReplayOwner>>,
    interface_relations: Vec<Option<ReplayOwner>>,
    override_checks: Vec<Option<ReplayOwner>>,
}

fn push_replay_owner(
    sidecar: &mut Option<Box<DeferredReplayOwners>>,
    select: impl Fn(&mut DeferredReplayOwners) -> &mut Vec<Option<ReplayOwner>>,
    prior_len: usize,
    owner: Option<ReplayOwner>,
) {
    if sidecar.is_none() && owner.is_none() {
        return;
    }
    let owners = select(sidecar.get_or_insert_with(|| Box::new(DeferredReplayOwners::default())));
    if owners.len() < prior_len {
        owners.resize(prior_len, None);
    }
    owners.push(owner);
}

fn merge_replay_owner_column(
    target: &mut Option<Box<DeferredReplayOwners>>,
    source: &mut Option<Box<DeferredReplayOwners>>,
    select: impl Fn(&mut DeferredReplayOwners) -> &mut Vec<Option<ReplayOwner>>,
    target_len: usize,
    source_len: usize,
) {
    if target.is_none() && source.is_none() {
        return;
    }
    let target_column =
        select(target.get_or_insert_with(|| Box::new(DeferredReplayOwners::default())));
    target_column.resize(target_len, None);
    match source.as_mut() {
        Some(source) => {
            let source_column = select(source);
            source_column.resize(source_len, None);
            target_column.append(source_column);
        }
        None => target_column.resize(target_len + source_len, None),
    }
}

impl<Ticket: Copy> CheckerEffects<Ticket> {
    pub(in crate::check::checker) fn new(owner: Ticket) -> Self {
        Self {
            records: CheckerRecordBatch::new(owner),
            activated: false,
            obligations: Vec::new(),
            constraint_checks: Vec::new(),
            interface_relations: Vec::new(),
            override_checks: Vec::new(),
            replay_owners: None,
            nested: Vec::new(),
        }
    }
}

impl CheckerEffects<UserRecordTicket> {
    pub(in crate::check::checker) fn from_records(records: CandidateEffects) -> Self {
        let (owner, records) = records.into_parts();
        Self {
            records: CheckerRecordBatch::from_parts(owner, records),
            activated: true,
            obligations: Vec::new(),
            constraint_checks: Vec::new(),
            interface_relations: Vec::new(),
            override_checks: Vec::new(),
            replay_owners: None,
            nested: Vec::new(),
        }
    }
}

impl<Ticket: Copy + PartialEq> CheckerEffects<Ticket> {
    pub(in crate::check::checker) fn activate(&mut self) {
        self.activated = true;
    }

    pub(in crate::check::checker) const fn is_activated(&self) -> bool {
        self.activated
    }

    pub(in crate::check::checker) fn merge(&mut self, mut child: CheckerEffects<Ticket>) {
        self.activated |= child.activated;
        merge_replay_owner_column(
            &mut self.replay_owners,
            &mut child.replay_owners,
            |owners| &mut owners.obligations,
            self.obligations.len(),
            child.obligations.len(),
        );
        merge_replay_owner_column(
            &mut self.replay_owners,
            &mut child.replay_owners,
            |owners| &mut owners.constraint_checks,
            self.constraint_checks.len(),
            child.constraint_checks.len(),
        );
        merge_replay_owner_column(
            &mut self.replay_owners,
            &mut child.replay_owners,
            |owners| &mut owners.interface_relations,
            self.interface_relations.len(),
            child.interface_relations.len(),
        );
        merge_replay_owner_column(
            &mut self.replay_owners,
            &mut child.replay_owners,
            |owners| &mut owners.override_checks,
            self.override_checks.len(),
            child.override_checks.len(),
        );
        self.records.merge(child.records);
        self.obligations.extend(child.obligations);
        self.constraint_checks.extend(child.constraint_checks);
        self.interface_relations.extend(child.interface_relations);
        self.override_checks.extend(child.override_checks);
        self.nested.extend(child.nested);
    }

    pub(in crate::check::checker) fn push_obligation(
        &mut self,
        obligation: DeferredRelationObligation,
        owner: Option<ReplayOwner>,
    ) {
        push_replay_owner(
            &mut self.replay_owners,
            |owners| &mut owners.obligations,
            self.obligations.len(),
            owner,
        );
        self.obligations.push(obligation);
    }

    pub(in crate::check::checker) fn push_constraint_check(
        &mut self,
        check: ConstraintCheckObligation,
        owner: Option<ReplayOwner>,
    ) {
        push_replay_owner(
            &mut self.replay_owners,
            |owners| &mut owners.constraint_checks,
            self.constraint_checks.len(),
            owner,
        );
        self.constraint_checks.push(check);
    }

    pub(in crate::check::checker) fn push_interface_relation(
        &mut self,
        relation: InterfaceRelationObligation,
        owner: Option<ReplayOwner>,
    ) {
        push_replay_owner(
            &mut self.replay_owners,
            |owners| &mut owners.interface_relations,
            self.interface_relations.len(),
            owner,
        );
        self.interface_relations.push(relation);
    }

    pub(in crate::check::checker) fn push_override(
        &mut self,
        check: OverrideCheck,
        owner: Option<ReplayOwner>,
    ) {
        push_replay_owner(
            &mut self.replay_owners,
            |owners| &mut owners.override_checks,
            self.override_checks.len(),
            owner,
        );
        self.override_checks.push(check);
    }

    pub(in crate::check::checker) fn take_constraint_checks(
        &mut self,
    ) -> (
        Vec<ConstraintCheckObligation>,
        Option<Vec<Option<ReplayOwner>>>,
    ) {
        let values = std::mem::take(&mut self.constraint_checks);
        let owners = self
            .replay_owners
            .as_mut()
            .map(|owners| std::mem::take(&mut owners.constraint_checks));
        (values, owners)
    }

    pub(in crate::check::checker) fn take_interface_relations(
        &mut self,
    ) -> (
        Vec<InterfaceRelationObligation>,
        Option<Vec<Option<ReplayOwner>>>,
    ) {
        let values = std::mem::take(&mut self.interface_relations);
        let owners = self
            .replay_owners
            .as_mut()
            .map(|owners| std::mem::take(&mut owners.interface_relations));
        (values, owners)
    }

    pub(in crate::check::checker) fn take_obligations(
        &mut self,
    ) -> (
        Vec<DeferredRelationObligation>,
        Option<Vec<Option<ReplayOwner>>>,
    ) {
        let values = std::mem::take(&mut self.obligations);
        let owners = self
            .replay_owners
            .as_mut()
            .map(|owners| std::mem::take(&mut owners.obligations));
        (values, owners)
    }

    pub(in crate::check::checker) fn take_override_checks(
        &mut self,
    ) -> (Vec<OverrideCheck>, Option<Vec<Option<ReplayOwner>>>) {
        let values = std::mem::take(&mut self.override_checks);
        let owners = self
            .replay_owners
            .as_mut()
            .map(|owners| std::mem::take(&mut owners.override_checks));
        (values, owners)
    }
}

/// Dense ticket-key lookup for exactly-once lexical-effect coalescing.
pub(in crate::check::checker) struct PendingEffectSlots {
    events: Vec<Vec<Option<usize>>>,
}

impl PendingEffectSlots {
    pub(in crate::check::checker) fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub(in crate::check::checker) fn get_or_insert(
        &mut self,
        (event, record): (usize, usize),
        insert: impl FnOnce() -> usize,
    ) -> (usize, bool) {
        if self.events.len() <= event {
            self.events.resize_with(event + 1, Vec::new);
        }
        let records = &mut self.events[event];
        if records.len() <= record {
            records.resize(record + 1, None);
        }
        match records[record] {
            Some(index) => (index, false),
            None => {
                let index = insert();
                records[record] = Some(index);
                (index, true)
            }
        }
    }
}

/// One observation of a declaration slot while an argument frame is open.
///
/// The log these form is what makes an argument-walk memo sound (backlog `95`): a
/// memoized walk records the slots it *read* before it wrote them — its dependency on
/// the incoming environment — and the slots it *wrote*, which are replayed when the
/// walk is skipped.
#[derive(Clone, Copy)]
struct DeclEvent {
    id: ValueStorageId,
    /// The slot's value before a write; equal to `value` for a read.
    previous: Option<TypeId>,
    value: Option<TypeId>,
    write: bool,
}

/// One slot's net movement across a stretch of walking: what it held when the stretch
/// began and what it holds now. A slot written and then restored has `previous ==
/// value` and is not a net write at all, which is what keeps a discarded contextual
/// excursion from being replayed as a real rebinding.
#[derive(Clone, Copy)]
pub(in crate::check::checker) struct NetDeclWrite {
    pub(in crate::check::checker) id: ValueStorageId,
    pub(in crate::check::checker) previous: Option<TypeId>,
    pub(in crate::check::checker) value: Option<TypeId>,
}

thread_local! {
    /// The live declaration observation log, `None` when closed. It is thread-local
    /// rather than a field because [`DeclTypes`] is also the process-wide frozen
    /// library base and must stay `Sync`; exactly one table is live per checking
    /// thread, and the log is open only for the outermost in-flight call/`new`
    /// argument frame.
    static DECL_OBSERVATIONS: RefCell<Option<Vec<DeclEvent>>> = const { RefCell::new(None) };
}

fn observing_declarations() -> bool {
    DECL_OBSERVATIONS.with(|log| log.borrow().is_some())
}

fn observe_declaration(
    id: ValueStorageId,
    previous: Option<TypeId>,
    value: Option<TypeId>,
    write: bool,
) {
    DECL_OBSERVATIONS.with(|log| {
        if let Some(log) = log.borrow_mut().as_mut() {
            log.push(DeclEvent {
                id,
                previous,
                value,
                write,
            });
        }
    });
}

/// Per-declaration computed types, indexed by [`ValueStorageId`]. `None` means a
/// declaration whose type could not be computed (out of subset); a reference to
/// it resolves to the error type defensively.
#[derive(Clone)]
pub(in crate::check::checker) struct DeclTypes {
    base: Arc<[Option<TypeId>]>,
    overrides: Option<FxHashMap<ValueStorageId, Option<TypeId>>>,
    local: Vec<Option<TypeId>>,
    sealed: bool,
}

impl DeclTypes {
    pub(in crate::check::checker) fn new(count: u32) -> Self {
        DeclTypes {
            base: Arc::from([]),
            overrides: None,
            local: vec![None; count as usize],
            sealed: false,
        }
    }

    /// The single mutator: every declaration-type change in the checker reaches the
    /// table through here, so the observation log cannot fall behind.
    fn put(&mut self, id: ValueStorageId, value: Option<TypeId>) {
        if id.index() < self.base.len() {
            let previous = self.untracked_get(id);
            let Some(overrides) = self.overrides.as_mut() else {
                return;
            };
            if previous != value {
                overrides.insert(id, value);
                observe_declaration(id, previous, value, true);
            }
            return;
        };
        let Some(slot) = id
            .index()
            .checked_sub(self.base.len())
            .and_then(|index| self.local.get_mut(index))
        else {
            return;
        };
        let previous = *slot;
        if previous == value {
            return;
        }
        *slot = value;
        observe_declaration(id, previous, value, true);
    }

    pub(in crate::check::checker) fn set(&mut self, id: ValueStorageId, ty: TypeId) {
        self.put(id, Some(ty));
    }

    /// Read a slot without recording the read. Reserved for the memo's own validity
    /// check, which asks what a slot holds *now* rather than depending on it.
    fn untracked_get(&self, id: ValueStorageId) -> Option<TypeId> {
        let index = id.index();
        if index < self.base.len() {
            return self
                .overrides
                .as_ref()
                .and_then(|overrides| overrides.get(&id).copied())
                .unwrap_or_else(|| self.base.get(index).copied().flatten());
        }
        self.local.get(index - self.base.len()).copied().flatten()
    }

    pub(in crate::check::checker) fn is_unoverridden_frozen_slot(
        &self,
        id: ValueStorageId,
    ) -> bool {
        id.index() < self.base.len()
            && self
                .overrides
                .as_ref()
                .is_some_and(|overrides| !overrides.contains_key(&id))
    }

    /// Open/close the observation log. The checker keeps it open for exactly the
    /// duration of the outermost in-flight call/`new` argument frame. Opening resets,
    /// so a log abandoned by a panicking check cannot leak into the next one.
    pub(in crate::check::checker) fn open_log() {
        DECL_OBSERVATIONS.with(|log| {
            let mut log = log.borrow_mut();
            debug_assert!(log.is_none(), "argument observation logs do not nest");
            *log = Some(Vec::new());
        });
    }

    pub(in crate::check::checker) fn close_log() {
        DECL_OBSERVATIONS.with(|log| *log.borrow_mut() = None);
    }

    /// Where the log stands now, so a walk can later ask what it read and wrote.
    pub(in crate::check::checker) fn log_mark() -> usize {
        DECL_OBSERVATIONS.with(|log| log.borrow().as_ref().map_or(0, Vec::len))
    }

    /// The slots a walk depended on: those it read **before** it wrote them. A slot
    /// the walk wrote first is its own output, not an input, so re-running the walk in
    /// an environment that disagrees only there would still take the same path.
    pub(in crate::check::checker) fn dependencies_since(
        mark: usize,
    ) -> Vec<(ValueStorageId, Option<TypeId>)> {
        let mut seen: FxHashSet<ValueStorageId> = FxHashSet::default();
        let mut dependencies = Vec::new();
        Self::with_log_tail(mark, |tail| {
            for event in tail {
                if !seen.insert(event.id) {
                    continue;
                }
                if !event.write {
                    dependencies.push((event.id, event.value));
                }
            }
        });
        dependencies
    }

    /// The net effect of everything written since `mark`: each touched slot once,
    /// carrying its final value, in first-touch order. Replaying this on the state the
    /// mark was taken in reproduces the current state exactly.
    pub(in crate::check::checker) fn net_writes_since(mark: usize) -> Vec<NetDeclWrite> {
        let mut order: Vec<ValueStorageId> = Vec::new();
        let mut movement: FxHashMap<ValueStorageId, (Option<TypeId>, Option<TypeId>)> =
            FxHashMap::default();
        Self::with_log_tail(mark, |tail| {
            for event in tail.iter().filter(|event| event.write) {
                match movement.get_mut(&event.id) {
                    Some(entry) => entry.1 = event.value,
                    None => {
                        movement.insert(event.id, (event.previous, event.value));
                        order.push(event.id);
                    }
                }
            }
        });
        order
            .into_iter()
            .filter_map(|id| {
                movement
                    .get(&id)
                    .map(|&(previous, value)| (id, previous, value))
            })
            .filter(|&(_, previous, value)| previous != value)
            .map(|(id, previous, value)| NetDeclWrite {
                id,
                previous,
                value,
            })
            .collect()
    }

    fn with_log_tail(mark: usize, mut visit: impl FnMut(&[DeclEvent])) {
        DECL_OBSERVATIONS.with(|log| {
            if let Some(tail) = log.borrow().as_ref().and_then(|log| log.get(mark..)) {
                visit(tail);
            }
        });
    }

    /// Whether every slot a memoized walk depended on still holds the value it saw.
    pub(in crate::check::checker) fn dependencies_hold(
        &self,
        dependencies: &[(ValueStorageId, Option<TypeId>)],
    ) -> bool {
        dependencies
            .iter()
            .all(|&(id, value)| self.untracked_get(id) == value)
    }

    /// Re-observe a memoized walk's dependencies, so an enclosing walk inherits them
    /// as its own — the skipped walk read those slots, and the enclosing walk's memo
    /// entry must say so.
    pub(in crate::check::checker) fn replay_dependencies(
        dependencies: &[(ValueStorageId, Option<TypeId>)],
    ) {
        for &(id, value) in dependencies {
            observe_declaration(id, value, value, false);
        }
    }

    /// Republish a memoized walk's declaration writes, so skipping the walk leaves the
    /// same bindings behind that performing it would have.
    pub(in crate::check::checker) fn apply_writes(&mut self, writes: &[NetDeclWrite]) {
        for write in writes {
            self.put(write.id, write.value);
        }
    }

    /// Restore a snapshot taken at `mark`, recording the undo in the log so the log
    /// still describes the table's net movement across the excursion.
    pub(in crate::check::checker) fn restore(&mut self, saved: Self, mark: usize) {
        let undo = Self::net_writes_since(mark);
        *self = saved;
        for write in undo {
            debug_assert_eq!(write.previous, self.untracked_get(write.id));
            observe_declaration(write.id, write.value, write.previous, true);
        }
    }

    pub(in crate::check::checker) fn get(&self, id: ValueStorageId) -> Option<TypeId> {
        let value = self.untracked_get(id);
        // Shared-prefix slots are immutable, but a sparse collision epoch can replace
        // one after a memoized walk reads it.
        if id.index() >= self.base.len() || self.overrides.is_some() {
            observe_declaration(id, value, value, false);
        }
        value
    }

    pub(in crate::check::checker) fn resize(&mut self, count: u32) {
        debug_assert!(
            !observing_declarations(),
            "declaration storage never grows inside an argument frame"
        );
        assert!(
            count as usize >= self.len(),
            "declaration type storage grows only by suffix"
        );
        while self.len() < count as usize {
            self.local.push(None);
        }
    }

    pub(in crate::check::checker) fn len(&self) -> usize {
        self.base.len() + self.local.len()
    }

    pub(in crate::check::checker) fn snapshot_slots(&self) -> Vec<Option<TypeId>> {
        let mut slots = self
            .base
            .iter()
            .chain(&self.local)
            .copied()
            .collect::<Vec<_>>();
        if let Some(overrides) = &self.overrides {
            for (id, value) in overrides {
                if let Some(slot) = slots.get_mut(id.index()) {
                    *slot = *value;
                }
            }
        }
        slots
    }

    #[cfg(test)]
    pub(in crate::check::checker) fn from_snapshot_slots(
        slots: Vec<Option<TypeId>>,
        expected_len: u32,
    ) -> Result<Self, &'static str> {
        if slots.len()
            != usize::try_from(expected_len)
                .map_err(|_| "snapshot declaration count does not fit usize")?
        {
            return Err("snapshot declaration type slots do not match binder storage count");
        }
        Ok(Self {
            base: Arc::from([]),
            overrides: None,
            local: slots,
            sealed: false,
        })
    }

    pub(in crate::check::checker) fn freeze_as_base(&mut self) -> Result<(), &'static str> {
        debug_assert!(
            !observing_declarations(),
            "declaration storage never seals inside an argument frame"
        );
        if self.sealed || !self.base.is_empty() {
            return Err("declaration types are already sealed");
        }
        self.base = Arc::from(std::mem::take(&mut self.local));
        self.sealed = true;
        Ok(())
    }

    pub(in crate::check::checker) fn fork_delta(&self) -> Result<Self, &'static str> {
        debug_assert!(
            !observing_declarations(),
            "declaration storage never forks inside an argument frame"
        );
        if !self.sealed || !self.local.is_empty() {
            return Err("declaration type base is not sealed");
        }
        Ok(Self {
            base: Arc::clone(&self.base),
            overrides: None,
            local: Vec::new(),
            sealed: true,
        })
    }

    pub(in crate::check::checker) fn fork_sparse_delta(&self) -> Result<Self, &'static str> {
        let mut delta = self.fork_delta()?;
        delta.overrides = Some(FxHashMap::default());
        Ok(delta)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(in crate::check::checker) fn shares_base_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.base, &other.base)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(in crate::check::checker) fn local_len(&self) -> usize {
        self.local.len()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(in crate::check::checker) fn local_slots(
        &self,
    ) -> impl Iterator<Item = (ValueStorageId, Option<TypeId>)> + '_ {
        let base_len = self.base.len();
        self.local
            .iter()
            .copied()
            .enumerate()
            .map(move |(index, ty)| {
                let id = u32::try_from(base_len + index).expect("value storage id fits u32");
                (ValueStorageId(id), ty)
            })
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(in crate::check::checker) fn changed_slots(
        &self,
    ) -> impl Iterator<Item = (ValueStorageId, Option<TypeId>)> + '_ {
        self.overrides
            .iter()
            .flat_map(|overrides| overrides.iter().map(|(&id, &ty)| (id, ty)))
            .chain(self.local_slots())
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(in crate::check::checker) fn replacement_len(&self) -> usize {
        self.overrides.as_ref().map_or(0, FxHashMap::len)
    }
}

/// A function declaration's callable signature, reserved before statement bodies
/// are checked. The reservation owns the stable generic ids and lowered signature;
/// body checking later fills only an unannotated return type.
pub(in crate::check::checker) struct FunctionSurface<Ticket: Copy = UserRecordTicket> {
    pub(in crate::check::checker) receiver: Option<TypeId>,
    pub(in crate::check::checker) params: Vec<crate::types::repr::ParameterType>,
    pub(in crate::check::checker) generic_params: Vec<crate::types::repr::GenericTypeParam>,
    pub(in crate::check::checker) type_param_frame: FxHashMap<String, TypeId>,
    pub(in crate::check::checker) declared_return: Option<TypeId>,
    pub(in crate::check::checker) function_ty: TypeId,
    /// Preallocated declaration owners; expression/arrow surfaces emit into the
    /// enclosing active lexical effect frame and therefore carry no tickets.
    pub(in crate::check::checker) tickets: Option<CallableTickets<Ticket>>,
}

/// An explicit `var` annotation lowered before executable checking. The type makes
/// the hoisted binding usable, while records wait for the declaration position.
pub(in crate::check::checker) struct VarAnnotationSurface {
    pub(in crate::check::checker) annotation: Option<TypeId>,
}

/// Whether a shared `var` declaration type is only a forward annotation, comes
/// from its first source declarator, or belongs to an earlier non-`var` binding.
pub(in crate::check::checker) enum VarValueTypeState {
    Provisional,
    Source,
    Existing,
}

/// A top-level type declaration's reserve-then-fill plan, indexed by type-space
/// [`TypeGroupId`]. Generic declarations carry ordered type-parameter ids and resolve to
/// templates instantiated by substitution.
#[derive(Clone)]
pub(in crate::check::checker) struct InterfaceFragment<'ast> {
    pub(in crate::check::checker) declaration: DeclId,
    pub(in crate::check::checker) scope: ScopeId,
    pub(in crate::check::checker) param_decl: Option<&'ast TSTypeParameterDeclaration<'ast>>,
    /// Fragment-local recovery frame: matching name/position shares the canonical id;
    /// renamed and excess parameters retain distinct binders as typed alternatives.
    pub(in crate::check::checker) params: Vec<TypeParamId>,
    pub(in crate::check::checker) members: &'ast [oxc_ast::ast::TSSignature<'ast>],
    pub(in crate::check::checker) extends: &'ast [TSInterfaceHeritage<'ast>],
}

/// One exact type-parameter identity retained from a class/interface header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct NamedTypeParamBinding {
    pub(in crate::check::checker) name: String,
    pub(in crate::check::checker) id: TypeParamId,
}

/// Query-free reservation facts for one exact class/interface declaration header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct HeaderFragmentBinding {
    pub(in crate::check::checker) declaration: DeclId,
    pub(in crate::check::checker) parameters: Vec<NamedTypeParamBinding>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum TypeParameterMetadataState {
    Absent,
    Ready(TypeId),
    Poisoned,
    Unsupported,
}

impl TypeParameterMetadataState {
    pub(in crate::check::checker) fn ready(self) -> Option<TypeId> {
        match self {
            Self::Ready(ty) => Some(ty),
            Self::Absent | Self::Poisoned | Self::Unsupported => None,
        }
    }

    pub(in crate::check::checker) fn is_supplied(self) -> bool {
        !matches!(self, Self::Absent)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct TypeGroupParameterDescriptors {
    pub(in crate::check::checker) constraints: Vec<TypeParameterMetadataState>,
    pub(in crate::check::checker) defaults: Vec<TypeParameterMetadataState>,
}

#[derive(Clone)]
pub(in crate::check::checker) enum TypeDecl<'ast> {
    /// An interface reserves an object id, then fills own and inherited members into it.
    /// Generic interfaces fill a template that type references instantiate later.
    Interface {
        declaration: DeclId,
        scope: ScopeId,
        reserved: TypeId,
        params: Vec<TypeParamId>,
        recovery_params: Vec<TypeParamId>,
        recovery_names: Vec<String>,
        recovery_defaults: Vec<PublishedTypeParameterDefault>,
        param_slots: BTreeMap<(usize, String), TypeParamId>,
        conflict_alternatives: Vec<InterfaceTypedAlternative>,
        defaults: Vec<Option<TypeId>>,
        parameter_descriptors: Option<TypeGroupParameterDescriptors>,
        param_decl: Option<&'ast TSTypeParameterDeclaration<'ast>>,
        /// Heritage clauses are composed into the reserved object during fill.
        extends: &'ast [TSInterfaceHeritage<'ast>],
        /// Every declaration fragment in canonical source order shares `reserved`.
        fragments: Vec<InterfaceFragment<'ast>>,
    },
    /// A transparent alias lowers on demand. Template placeholders keep recursive
    /// conditional, mapped, and object-literal alias shapes from expanding inline.
    Alias {
        declaration: DeclId,
        scope: ScopeId,
        annotation: &'ast TSType<'ast>,
        params: Vec<TypeParamId>,
        defaults: Vec<Option<TypeId>>,
        param_decl: Option<&'ast TSTypeParameterDeclaration<'ast>>,
        resolving: bool,
        /// Reserved conditional template id, seeded before the body is lowered.
        conditional_template: Option<TypeId>,
        /// Reserved mapped template id, kept lazy for recursive mapped aliases.
        mapped_template: Option<TypeId>,
        /// Reserved object id for legal member recursion in non-generic object aliases.
        object_template: Option<TypeId>,
        /// Alias name for circular-alias diagnostics.
        name: String,
        /// Alias name span for circular-alias diagnostics.
        name_span: Span,
    },
    /// A class reserves only stable nominal/application metadata. Publication builds
    /// immutable instance/static templates after every surface child is lowered.
    Class {
        declaration: DeclId,
        scope: ScopeId,
        class_id: ClassId,
        /// Complete application/recovery frame for the merged type group.
        params: Vec<TypeParamId>,
        /// Exact lexical binders declared by the class header.
        class_params: Vec<TypeParamId>,
        recovery_names: Vec<String>,
        recovery_defaults: Vec<PublishedTypeParameterDefault>,
        param_slots: BTreeMap<(usize, String), TypeParamId>,
        conflict_alternatives: Vec<InterfaceTypedAlternative>,
        parameter_descriptors: Option<TypeGroupParameterDescriptors>,
        param_decl: Option<&'ast TSTypeParameterDeclaration<'ast>>,
        class: &'ast Class<'ast>,
        /// Interface declarations owned by this class surface, in source order.
        interfaces: Vec<InterfaceFragment<'ast>>,
        /// Exact class/interface header binders, in source order.
        header_fragments: Vec<HeaderFragmentBinding>,
    },
    /// Any other unsupported multi-kind group has no permissive semantic surface.
    Unavailable { declaration: DeclId },
    /// A declaration already resolved by an earlier compilation unit, such as the prelude.
    Resolved {
        params: Vec<TypeParamId>,
        defaults: Vec<PublishedTypeParameterDefault>,
    },
}

#[derive(Clone)]
pub(in crate::check::checker) struct PublishedTypeDecl {
    pub(in crate::check::checker) params: Vec<TypeParamId>,
    pub(in crate::check::checker) defaults: Vec<PublishedTypeParameterDefault>,
}

pub(in crate::check::checker) enum TypeDeclView<'table, 'ast> {
    Published(&'table PublishedTypeDecl),
    Local(&'table TypeDecl<'ast>),
}

pub(in crate::check::checker) struct TypeDeclTable<'ast> {
    published: LayeredVec<PublishedTypeDecl>,
    replacements: FxHashMap<usize, TypeDecl<'ast>>,
    local: Vec<TypeDecl<'ast>>,
}

/// Copying the declaration table is per-row deep work, so surface lowering must borrow it
/// rather than clone it — `surface_lowering_copy_spec` pins that to zero copied rows.
impl<'ast> Clone for TypeDeclTable<'ast> {
    fn clone(&self) -> Self {
        #[cfg(test)]
        record_copied_decl_rows_for_test(self.published.local_len() + self.local.len());
        Self {
            published: self.published.clone(),
            replacements: self.replacements.clone(),
            local: self.local.clone(),
        }
    }
}

#[cfg(test)]
thread_local! {
    static COPIED_DECL_ROWS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// The base layer is shared by `Arc`, so only the local rows are actually deep-copied.
#[cfg(test)]
fn record_copied_decl_rows_for_test(rows: usize) {
    if rows > 0 {
        let rows = u64::try_from(rows).unwrap_or(u64::MAX);
        COPIED_DECL_ROWS.set(COPIED_DECL_ROWS.get().saturating_add(rows));
    }
}

#[cfg(test)]
pub(in crate::check::checker) struct CopiedDeclRowScopeForTest(u64);

#[cfg(test)]
impl CopiedDeclRowScopeForTest {
    pub(in crate::check::checker) fn start() -> Self {
        Self(COPIED_DECL_ROWS.get())
    }

    pub(in crate::check::checker) fn finish(self) -> u64 {
        COPIED_DECL_ROWS.get().saturating_sub(self.0)
    }
}

impl<'ast> TypeDeclTable<'ast> {
    pub(in crate::check::checker) fn with_published(
        published: LayeredVec<PublishedTypeDecl>,
    ) -> Self {
        Self {
            published,
            replacements: FxHashMap::default(),
            local: Vec::new(),
        }
    }

    pub(in crate::check::checker) fn len(&self) -> usize {
        self.published.len() + self.local.len()
    }

    pub(in crate::check::checker) fn get(&self, index: usize) -> Option<&TypeDecl<'ast>> {
        if index < self.published.len() {
            return self.replacements.get(&index);
        }
        self.local.get(index.checked_sub(self.published.len())?)
    }

    pub(in crate::check::checker) fn view(&self, index: usize) -> Option<TypeDeclView<'_, 'ast>> {
        if let Some(replacement) = self.replacements.get(&index) {
            Some(TypeDeclView::Local(replacement))
        } else if let Some(published) = self.published.get(index) {
            Some(TypeDeclView::Published(published))
        } else {
            self.get(index).map(TypeDeclView::Local)
        }
    }

    pub(in crate::check::checker) fn get_mut(
        &mut self,
        index: usize,
    ) -> Option<&mut TypeDecl<'ast>> {
        if index < self.published.len() {
            if !self.published.prefix_overrides_enabled() {
                return None;
            }
            let published = self.published.get(index)?;
            return Some(
                self.replacements
                    .entry(index)
                    .or_insert(TypeDecl::Resolved {
                        params: published.params.clone(),
                        defaults: published.defaults.clone(),
                    }),
            );
        }
        let local_index = index.checked_sub(self.published.len())?;
        self.local.get_mut(local_index)
    }

    pub(in crate::check::checker) fn push(&mut self, declaration: TypeDecl<'ast>) {
        self.local.push(declaration);
    }

    pub(in crate::check::checker) fn iter(
        &self,
    ) -> impl DoubleEndedIterator<Item = &TypeDecl<'ast>> + ExactSizeIterator + Clone {
        self.local.iter()
    }

    pub(in crate::check::checker) fn changed_entries(&self) -> Vec<(usize, &TypeDecl<'ast>)> {
        let mut entries = self
            .replacements
            .iter()
            .map(|(index, declaration)| (*index, declaration))
            .chain(
                self.local
                    .iter()
                    .enumerate()
                    .map(|(index, declaration)| (self.published.len() + index, declaration)),
            )
            .collect::<Vec<_>>();
        entries.sort_by_key(|(index, _)| *index);
        entries
    }

    pub(in crate::check::checker) fn has_replacement(&self, index: usize) -> bool {
        self.replacements.contains_key(&index)
    }

    pub(in crate::check::checker) fn replacement_indices(&self) -> Vec<usize> {
        let mut indices = self.replacements.keys().copied().collect::<Vec<_>>();
        indices.sort_unstable();
        indices
    }

    pub(in crate::check::checker) fn published_len(&self) -> usize {
        self.published.len()
    }

    pub(in crate::check::checker) fn published_params(
        &self,
        index: usize,
    ) -> Option<&[TypeParamId]> {
        Some(self.published.get(index)?.params.as_slice())
    }
}

impl<'ast> From<Vec<TypeDecl<'ast>>> for TypeDeclTable<'ast> {
    fn from(local: Vec<TypeDecl<'ast>>) -> Self {
        Self {
            published: LayeredVec::default(),
            replacements: FxHashMap::default(),
            local,
        }
    }
}

impl<'ast> Index<usize> for TypeDeclTable<'ast> {
    type Output = TypeDecl<'ast>;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("type declaration index in range")
    }
}

impl<'ast> IndexMut<usize> for TypeDeclTable<'ast> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.get_mut(index)
            .expect("published type declarations are immutable")
    }
}

#[derive(Clone)]
pub(in crate::check::checker) struct TypeResolvedTable {
    published: LayeredVec<Option<TypeId>>,
    local: Vec<Option<TypeId>>,
}

impl TypeResolvedTable {
    pub(in crate::check::checker) fn with_published(published: LayeredVec<Option<TypeId>>) -> Self {
        Self {
            published,
            local: Vec::new(),
        }
    }

    pub(in crate::check::checker) fn resize(&mut self, len: usize, value: Option<TypeId>) {
        assert!(len >= self.published.len());
        self.local.resize(len - self.published.len(), value);
    }

    pub(in crate::check::checker) fn get(&self, index: usize) -> Option<&Option<TypeId>> {
        self.published
            .get(index)
            .or_else(|| self.local.get(index.checked_sub(self.published.len())?))
    }

    pub(in crate::check::checker) fn get_mut(
        &mut self,
        index: usize,
    ) -> Option<&mut Option<TypeId>> {
        if index < self.published.len() {
            return self.published.get_mut_local(index);
        }
        self.local.get_mut(index.checked_sub(self.published.len())?)
    }
}

impl From<Vec<Option<TypeId>>> for TypeResolvedTable {
    fn from(local: Vec<Option<TypeId>>) -> Self {
        Self {
            published: LayeredVec::default(),
            local,
        }
    }
}

impl Index<usize> for TypeResolvedTable {
    type Output = Option<TypeId>;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("resolved type index in range")
    }
}

impl IndexMut<usize> for TypeResolvedTable {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.get_mut(index)
            .expect("published resolved types are immutable")
    }
}

#[derive(Clone)]
pub(in crate::check::checker) struct TemplateFillTable {
    published_len: usize,
    replacements: FxHashMap<usize, ClassFillState>,
    local: Vec<ClassFillState>,
}

impl TemplateFillTable {
    pub(in crate::check::checker) fn new(published_len: usize, local: Vec<ClassFillState>) -> Self {
        Self {
            published_len,
            replacements: FxHashMap::default(),
            local,
        }
    }

    pub(in crate::check::checker) fn install_replacement(
        &mut self,
        index: usize,
        state: ClassFillState,
    ) {
        if index < self.published_len {
            self.replacements.insert(index, state);
        }
    }

    pub(in crate::check::checker) fn get(&self, index: usize) -> Option<&ClassFillState> {
        static DONE: ClassFillState = ClassFillState::Done;
        if index < self.published_len {
            self.replacements.get(&index).or(Some(&DONE))
        } else {
            self.local.get(index - self.published_len)
        }
    }

    pub(in crate::check::checker) fn get_mut(
        &mut self,
        index: usize,
    ) -> Option<&mut ClassFillState> {
        if index < self.published_len {
            return self.replacements.get_mut(&index);
        }
        self.local.get_mut(index.checked_sub(self.published_len)?)
    }
}

impl Index<usize> for TemplateFillTable {
    type Output = ClassFillState;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("template fill index in range")
    }
}

impl IndexMut<usize> for TemplateFillTable {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.get_mut(index)
            .expect("published template fill state is immutable")
    }
}

/// A class's copyable `new` metadata, keyed by [`ValueStorageId`].
#[derive(Copy, Clone)]
pub(in crate::check::checker) struct ClassInfo {
    /// Constructor parameters as a function type.
    pub(in crate::check::checker) ctor: TypeId,
    /// Stable class identity used by access-control and nominal rules.
    pub(in crate::check::checker) class_id: ClassId,
    /// Whether directly constructing this class reports `TK2511`.
    pub(in crate::check::checker) is_abstract: bool,
    /// Constructor visibility for direct `new` accessibility checks.
    pub(in crate::check::checker) ctor_visibility: Visibility,
    /// Class that declares the constructor used for direct `new` accessibility checks.
    pub(in crate::check::checker) ctor_declaring_class: ClassId,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct PublishedClassNewMetadata {
    pub(in crate::check::checker) is_abstract: bool,
    pub(in crate::check::checker) ctor_visibility: Visibility,
    pub(in crate::check::checker) ctor_declaring_class: ClassId,
    pub(in crate::check::checker) has_source_overloads: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct PublishedClassValueBinding {
    pub(in crate::check::checker) class_id: ClassId,
    pub(in crate::check::checker) has_header_type_params: bool,
}

/// Total binder order for one exported value attached to a class namespace.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::check::checker) struct ClassNamespacePropertySourceOrder {
    pub(in crate::check::checker) source: SourceUnitKey,
    pub(in crate::check::checker) source_start: u32,
    pub(in crate::check::checker) declaration_ordinal: u32,
}

/// One class-attached namespace property and its exact diagnostic provenance.
#[derive(Clone, Debug)]
pub(in crate::check::checker) struct ClassNamespacePropertyPayload<Ticket: Copy = UserRecordTicket>
{
    pub(in crate::check::checker) property: PropertyType,
    pub(in crate::check::checker) declaration: DeclId,
    pub(in crate::check::checker) owner_span: Span,
    pub(in crate::check::checker) source_order: ClassNamespacePropertySourceOrder,
    pub(in crate::check::checker) owner: Ticket,
}

/// A class's fill progress, tracked per [`TypeDecl`] index.
/// `Filling` breaks `extends` cycles by treating the re-entered base as absent, so
/// lowering terminates; non-class indices start as `Done`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum ClassFillState {
    Pending,
    Filling,
    Done,
}

/// Query-invisible AST and fill state owned only by the construction phase.
/// Atomic publication consumes this object, so body/query phases cannot retain or
/// accidentally consult a mutable declaration draft.
pub(in crate::check::checker) struct ConstructionDrafts<'ast> {
    pub(in crate::check::checker) staged_published_classes:
        Option<crate::class_semantics::PublishedClasses>,
    pub(in crate::check::checker) type_group_construction: Option<TypeGroupConstruction>,
    pub(in crate::check::checker) type_decls: TypeDeclTable<'ast>,
    pub(in crate::check::checker) type_resolved: TypeResolvedTable,
    pub(in crate::check::checker) template_fill: TemplateFillTable,
}

#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::check::checker) struct EagerApplicationCacheMeasure {
    pub(in crate::check::checker) lookups: u64,
    pub(in crate::check::checker) hits: u64,
    pub(in crate::check::checker) misses: u64,
    pub(in crate::check::checker) insertions: u64,
    pub(in crate::check::checker) cycle_tainted_skips: u64,
    pub(in crate::check::checker) unready_bypasses: u64,
    pub(in crate::check::checker) unfinished_bypasses: u64,
    pub(in crate::check::checker) lazy_bypasses: u64,
}

#[cfg(test)]
pub(in crate::check::checker) type EagerApplicationCacheMeasureCollector =
    std::rc::Rc<std::cell::RefCell<EagerApplicationCacheMeasure>>;

#[cfg(test)]
thread_local! {
    static EAGER_APPLICATION_CACHE_MEASURE: std::cell::RefCell<Option<EagerApplicationCacheMeasureCollector>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(in crate::check::checker) struct EagerApplicationCacheMeasureScope {
    previous: Option<EagerApplicationCacheMeasureCollector>,
    _thread_affine: std::marker::PhantomData<std::rc::Rc<()>>,
}

#[cfg(test)]
impl Drop for EagerApplicationCacheMeasureScope {
    fn drop(&mut self) {
        EAGER_APPLICATION_CACHE_MEASURE.with(|measure| {
            measure.replace(self.previous.take());
        });
    }
}

#[cfg(test)]
pub(in crate::check::checker) fn start_eager_application_cache_measure(
) -> EagerApplicationCacheMeasureScope {
    let collector = std::rc::Rc::new(std::cell::RefCell::new(
        EagerApplicationCacheMeasure::default(),
    ));
    let previous = EAGER_APPLICATION_CACHE_MEASURE
        .with(|current| current.replace(Some(std::rc::Rc::clone(&collector))));
    EagerApplicationCacheMeasureScope {
        previous,
        _thread_affine: std::marker::PhantomData,
    }
}

#[cfg(test)]
pub(in crate::check::checker) fn eager_application_cache_measure(
) -> Option<EagerApplicationCacheMeasure> {
    let collector = EAGER_APPLICATION_CACHE_MEASURE.with(|current| current.borrow().clone())?;
    let measure = collector.borrow().clone();
    Some(measure)
}

#[cfg(test)]
pub(in crate::check::checker) fn capture_eager_application_cache_measure(
) -> Option<EagerApplicationCacheMeasureCollector> {
    EAGER_APPLICATION_CACHE_MEASURE.with(|current| current.borrow().clone())
}

#[cfg(test)]
pub(in crate::check::checker) fn record_eager_application_cache_measure(
    collector: &Option<EagerApplicationCacheMeasureCollector>,
    update: impl FnOnce(&mut EagerApplicationCacheMeasure),
) {
    if let Some(collector) = collector {
        update(&mut collector.borrow_mut());
    }
}

#[cfg(test)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct CycleTaintedApplicationCacheEntry {
    pub(in crate::check::checker) result: TypeId,
    pub(in crate::check::checker) first_run_visit_weight: u64,
}

#[cfg(test)]
pub(in crate::check::checker) type CycleTaintedApplicationCache =
    FxHashMap<(TypeId, Vec<(TypeParamId, TypeId)>), CycleTaintedApplicationCacheEntry>;

#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::check::checker) struct CycleTaintedApplicationCacheMeasure {
    pub(in crate::check::checker) lookups: u64,
    pub(in crate::check::checker) hits: u64,
    pub(in crate::check::checker) misses: u64,
    pub(in crate::check::checker) insertions: u64,
    pub(in crate::check::checker) clean_skips: u64,
    pub(in crate::check::checker) aborted_runs: u64,
    pub(in crate::check::checker) eligible_requests: u64,
    pub(in crate::check::checker) executed_runs: u64,
    pub(in crate::check::checker) clean_outcomes: u64,
    pub(in crate::check::checker) tainted_outcomes: u64,
    pub(in crate::check::checker) executed_visits: u64,
    pub(in crate::check::checker) memo_hits: u64,
    pub(in crate::check::checker) expanded_visits: u64,
    pub(in crate::check::checker) tainted_cache_hits: u64,
    pub(in crate::check::checker) tainted_cache_entries: u64,
    pub(in crate::check::checker) avoided_visits: u64,
    pub(in crate::check::checker) saturated: bool,
}

#[cfg(test)]
impl CycleTaintedApplicationCacheMeasure {
    fn checked_add(counter: &mut u64, value: u64, saturated: &mut bool) {
        match counter.checked_add(value) {
            Some(sum) => *counter = sum,
            None => {
                *counter = u64::MAX;
                *saturated = true;
            }
        }
    }

    pub(in crate::check::checker) fn eligible(&mut self) {
        Self::checked_add(&mut self.eligible_requests, 1, &mut self.saturated);
    }

    pub(in crate::check::checker) fn lookup(&mut self) {
        Self::checked_add(&mut self.lookups, 1, &mut self.saturated);
    }

    pub(in crate::check::checker) fn miss(&mut self) {
        Self::checked_add(&mut self.misses, 1, &mut self.saturated);
    }

    pub(in crate::check::checker) fn hit(&mut self, avoided_visits: u64) {
        Self::checked_add(&mut self.hits, 1, &mut self.saturated);
        Self::checked_add(&mut self.tainted_cache_hits, 1, &mut self.saturated);
        Self::checked_add(
            &mut self.avoided_visits,
            avoided_visits,
            &mut self.saturated,
        );
    }

    pub(in crate::check::checker) fn executed(
        &mut self,
        cycle_tainted: bool,
        visits: u64,
        memo_hits: u64,
    ) {
        Self::checked_add(&mut self.executed_runs, 1, &mut self.saturated);
        Self::checked_add(&mut self.executed_visits, visits, &mut self.saturated);
        Self::checked_add(&mut self.memo_hits, memo_hits, &mut self.saturated);
        let Some(expanded_visits) = visits.checked_sub(memo_hits) else {
            self.saturated = true;
            return;
        };
        Self::checked_add(
            &mut self.expanded_visits,
            expanded_visits,
            &mut self.saturated,
        );
        if cycle_tainted {
            Self::checked_add(&mut self.tainted_outcomes, 1, &mut self.saturated);
        } else {
            Self::checked_add(&mut self.clean_outcomes, 1, &mut self.saturated);
        }
    }

    pub(in crate::check::checker) fn clean_skip(&mut self) {
        Self::checked_add(&mut self.clean_skips, 1, &mut self.saturated);
    }

    pub(in crate::check::checker) fn insert(&mut self) {
        Self::checked_add(&mut self.insertions, 1, &mut self.saturated);
        Self::checked_add(&mut self.tainted_cache_entries, 1, &mut self.saturated);
    }

    pub(in crate::check::checker) fn abort(&mut self) {
        Self::checked_add(&mut self.aborted_runs, 1, &mut self.saturated);
    }
}

#[cfg(test)]
pub(in crate::check::checker) type CycleTaintedApplicationCacheMeasureCollector =
    std::rc::Rc<std::cell::RefCell<CycleTaintedApplicationCacheMeasure>>;

#[cfg(test)]
#[derive(Clone)]
pub(in crate::check::checker) struct CycleTaintedApplicationCacheCapture {
    pub(in crate::check::checker) collector: CycleTaintedApplicationCacheMeasureCollector,
    pub(in crate::check::checker) cache_enabled: bool,
}

#[cfg(test)]
thread_local! {
    static CYCLE_TAINTED_APPLICATION_CACHE_CAPTURE: std::cell::RefCell<Option<CycleTaintedApplicationCacheCapture>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(in crate::check::checker) struct CycleTaintedApplicationCacheMeasureScope {
    previous: Option<CycleTaintedApplicationCacheCapture>,
    _thread_affine: std::marker::PhantomData<std::rc::Rc<()>>,
}

#[cfg(test)]
impl Drop for CycleTaintedApplicationCacheMeasureScope {
    fn drop(&mut self) {
        CYCLE_TAINTED_APPLICATION_CACHE_CAPTURE.with(|capture| {
            capture.replace(self.previous.take());
        });
    }
}

#[cfg(test)]
fn start_cycle_tainted_application_cache_scope(
    cache_enabled: bool,
) -> CycleTaintedApplicationCacheMeasureScope {
    let capture = CycleTaintedApplicationCacheCapture {
        collector: std::rc::Rc::new(std::cell::RefCell::new(
            CycleTaintedApplicationCacheMeasure::default(),
        )),
        cache_enabled,
    };
    let previous =
        CYCLE_TAINTED_APPLICATION_CACHE_CAPTURE.with(|current| current.replace(Some(capture)));
    CycleTaintedApplicationCacheMeasureScope {
        previous,
        _thread_affine: std::marker::PhantomData,
    }
}

#[cfg(test)]
pub(in crate::check::checker) fn start_cycle_tainted_application_cache_measure(
) -> CycleTaintedApplicationCacheMeasureScope {
    start_cycle_tainted_application_cache_scope(true)
}

#[cfg(test)]
pub(in crate::check::checker) fn start_cycle_tainted_application_cache_baseline_measure(
) -> CycleTaintedApplicationCacheMeasureScope {
    start_cycle_tainted_application_cache_scope(false)
}

#[cfg(test)]
pub(in crate::check::checker) fn cycle_tainted_application_cache_measure(
) -> Option<CycleTaintedApplicationCacheMeasure> {
    let capture =
        CYCLE_TAINTED_APPLICATION_CACHE_CAPTURE.with(|current| current.borrow().clone())?;
    let measure = capture.collector.borrow().clone();
    Some(measure)
}

#[cfg(test)]
pub(in crate::check::checker) fn capture_cycle_tainted_application_cache_measure(
) -> Option<CycleTaintedApplicationCacheCapture> {
    CYCLE_TAINTED_APPLICATION_CACHE_CAPTURE.with(|current| current.borrow().clone())
}

#[cfg(test)]
pub(in crate::check::checker) fn record_cycle_tainted_application_cache_measure(
    collector: &Option<CycleTaintedApplicationCacheMeasureCollector>,
    update: impl FnOnce(&mut CycleTaintedApplicationCacheMeasure),
) {
    if let Some(collector) = collector {
        update(&mut collector.borrow_mut());
    }
}

/// The phase-1 working set threaded through the walk: everything the inference
/// pass writes to. Bundled into one struct so the many recursive `infer_*`/
/// `lower_*` helpers take a single `&mut` rather than a long, churn-prone argument
/// list.
pub(in crate::check::checker) struct Pass<'a, 'ast, Ticket: Copy + PartialEq = UserRecordTicket> {
    pub(in crate::check::checker) interner: &'a mut Interner,
    pub(in crate::check::checker) binder: &'a Binder,
    /// Completed eager generic applications in this pass's type universe.
    pub(in crate::check::checker) eager_application_cache:
        FxHashMap<(TypeId, Vec<(TypeParamId, TypeId)>), TypeId>,
    #[cfg(test)]
    pub(in crate::check::checker) eager_application_cache_measure:
        Option<EagerApplicationCacheMeasureCollector>,
    #[cfg(test)]
    pub(in crate::check::checker) cycle_tainted_application_cache:
        Option<CycleTaintedApplicationCache>,
    #[cfg(test)]
    pub(in crate::check::checker) cycle_tainted_application_cache_measure:
        Option<CycleTaintedApplicationCacheMeasureCollector>,
    #[cfg(test)]
    pub(in crate::check::checker) panic_before_cycle_tainted_application_cache_publish: bool,
    /// The module scope currently being checked.
    /// Disambiguates span-start keyed lookups for scopes and flow. Correct only
    /// while bodies are walked per module; lazy cross-module inference would
    /// desynchronize insert and lookup keys.
    pub(in crate::check::checker) current_module: ScopeId,
    /// Exact source identity, independent of reporting authority.
    pub(in crate::check::checker) current_source: SourceUnit,
    /// Present only while the source-backed library compiler generates replay evidence.
    pub(in crate::check::checker) replay_trace: Option<ReplayDependencyTrace>,
    /// Extra dependency reads consumed only by the compact private-replay plan.
    pub(in crate::check::checker) capture_compact_replay_dependencies: bool,
    pub(in crate::check::checker) compact_only_replay_edges:
        RefCell<BTreeSet<super::replay_index::ReplayReverseEdge>>,
    pub(in crate::check::checker) compact_demand_capture: Cell<bool>,
    pub(in crate::check::checker) compact_demand_added: Cell<bool>,
    /// Exact affected prefix closure admitted for this private collision epoch.
    pub(in crate::check::checker) private_collision_affected: BTreeSet<ReplayOwner>,
    #[cfg(any(test, feature = "test-utils"))]
    pub(in crate::check::checker) private_collision_state_drop_witness:
        Option<super::library_compiler::PrivateCollisionStateDropWitnessForTest>,
    /// Exact frozen value owner at each retained library binding site.
    pub(in crate::check::checker) private_library_value_owner_by_site:
        BTreeMap<(LibraryFileOrdinal, u32), Option<ValueStorageId>>,
    /// Affected type groups whose user augmentation could not be represented.
    pub(in crate::check::checker) private_collision_unavailable_type_groups: BTreeSet<TypeGroupId>,
    /// Library groups with user fragments in a fresh combined-source construction.
    pub(in crate::check::checker) combined_user_library_type_groups: BTreeSet<TypeGroupId>,
    /// The fresh combined-source pass is currently checking its user suffix.
    pub(in crate::check::checker) combined_user_source: bool,
    /// Library-owned value selected by each affected sealed root slot.
    pub(in crate::check::checker) private_collision_value_winners:
        FxHashMap<SymbolId, ValueStorageId>,
    pub(in crate::check::checker) private_collision_value_winners_by_name:
        FxHashMap<String, ValueStorageId>,
    pub(in crate::check::checker) combined_source_library_value_precedence: bool,
    /// Published `globalThis` object, augmented only inside a private collision epoch.
    pub(in crate::check::checker) global_object_type: Option<TypeId>,
    /// Hierarchical lexical/speculative output owners; only the outer owner commits.
    pub(in crate::check::checker) effect_stack: Vec<CheckerEffects<Ticket>>,
    /// Per in-flight call/`new`, the raw argument walk's held effects, indexed like
    /// `arg_types`. A re-walkable argument is walked once before candidate selection
    /// (for its type) and once after it (with the instantiated contextual target);
    /// only one of the two may commit, so the earlier walk is held here until the
    /// committed walk reports whether it superseded it — backlog `92`.
    pub(in crate::check::checker) provisional_argument_effects:
        Vec<Vec<ProvisionalArgumentWalk<Ticket>>>,
    /// Nesting depth of in-flight call/`new` argument frames. The argument-walk memos
    /// below live for exactly the outermost frame and are cleared as it closes, so a
    /// memoized answer can never outlive the walk region it was taken in.
    pub(in crate::check::checker) argument_walk_depth: u32,
    /// Memo for the raw (context-free) walk of a re-walkable call/`new` argument
    /// (backlog `95`). Re-executing an enclosing call — which is what a contextual
    /// re-walk of an enclosing argument does — otherwise re-walks this whole subtree.
    pub(in crate::check::checker) raw_argument_walk_memo:
        FxHashMap<ArgumentWalkKey, Vec<MemoizedArgumentWalk>>,
    /// Memo for the *effect-discarding* contextual argument re-walk (backlog `95`).
    /// Only that mode is eligible: its whole `CheckerEffects` is dropped, so the type
    /// it returns and the declaration bindings it leaves behind are its entire
    /// observable output. The walk that reports is never memoized.
    pub(in crate::check::checker) contextual_walk_memo:
        FxHashMap<ContextualWalkKey, Vec<MemoizedContextualWalk>>,
    /// Completed lexical owners awaiting deferred relation/override resolution.
    pub(in crate::check::checker) pending_effects: Vec<CheckerEffects<Ticket>>,
    /// O(1) owner-to-batch lookup over the reservation ledger's dense ticket keys.
    pub(in crate::check::checker) pending_effect_slots: PendingEffectSlots,
    pub(in crate::check::checker) pending_effect_key: fn(Ticket) -> (usize, usize),
    /// Prelude declaration lowering uses real lexical frames but discards their effects.
    pub(in crate::check::checker) suppress_effects: bool,
    /// Persistent lexical reservations built before class fill and body checking.
    pub(in crate::check::checker) lexical_events: LexicalReservations<Ticket>,
    /// Construction-only inherited view or the sole query-visible published snapshot.
    pub(in crate::check::checker) type_environment: TypeEnvironmentState<'ast>,
    /// Durable coordinator-owned projection, evaluation, and relation state.
    pub(in crate::check::checker) semantic_queries: SemanticQueryState,
    /// Source-provider-selected full-library roots for native-syntax member surfaces.
    pub(in crate::check::checker) library_semantic_identities: Option<LibrarySemanticIdentities>,
    /// Exact library-root array roles available before type-group publication.
    pub(in crate::check::checker) early_native_array_groups: Option<NativeArrayGroups>,
    /// Outermost type-declaration source that authorizes construction-only array roles.
    pub(in crate::check::checker) early_native_array_root_source: Option<SourceUnit>,
    /// Value roots authenticated before any library interface is filled.
    pub(in crate::check::checker) certified_library_values: CertifiedLibraryValues,
    /// Identity of the transitional minimal-prelude Array alias.
    pub(in crate::check::checker) lexical_array_alias: Option<TypeGroupId>,
    /// Frozen class parameter descriptors retained from the atomic publication.
    pub(in crate::check::checker) class_application_parameters:
        LayeredMap<ClassId, Vec<DraftClassTypeParameter<()>>>,
    /// Query-bearing class validation is held until type groups publish atomically.
    pub(in crate::check::checker) staged_class_validation: Option<StagedClassValidation<Ticket>>,
    /// Exact class callables retained by the one surface-lowering pass.
    pub(in crate::check::checker) retained_class_callables:
        BTreeMap<ClassId, Vec<RetainedClassCallable<Ticket>>>,
    /// Query-invisible own-member surfaces used only while checking class bodies.
    /// Poisoned classes stay exhausted through `published_classes`; these drafts keep
    /// independent body diagnostics from losing `this` and `static this`.
    pub(in crate::check::checker) class_body_views: BTreeMap<ClassId, BodyClassView>,
    /// Exact substituted base constructor for each published derived class.
    pub(in crate::check::checker) class_super_constructors: BTreeMap<ClassId, TypeId>,
    /// Owner-free `new` facts frozen before construction ASTs are dropped.
    pub(in crate::check::checker) class_new_metadata:
        LayeredMap<ClassId, PublishedClassNewMetadata>,
    /// Type-parameter scope stack.
    /// Frames are pushed only around their generic declaration, and innermost
    /// frames shadow binder type slots so `T` does not leak.
    pub(in crate::check::checker) type_param_scopes: Vec<FxHashMap<String, TypeId>>,
    /// Class binders hidden while a static member is lowered or checked. The
    /// enclosing frame remains present so an own method binder can shadow it.
    pub(in crate::check::checker) static_class_type_param_barriers: Vec<FxHashSet<TypeParamId>>,
    /// Running counter allocating a unique [`TypeParamId`] per declared type
    /// parameter across the whole module (the named-unique-id representation — see
    /// [`crate::types::repr::TypeParamId`]).
    pub(in crate::check::checker) next_type_param: u32,
    /// Parent class by stable [`ClassId`], built from resolvable `extends`.
    /// Used by `protected` access and independent of interned type identity.
    pub(in crate::check::checker) class_parents: LayeredMap<ClassId, ClassId>,
    /// One-step `const Alias = Class` origins. `infer_new` uses this only to retain
    /// the direct class's abstract and constructor-accessibility facts.
    pub(in crate::check::checker) class_value_aliases: LayeredMap<ValueStorageId, ValueStorageId>,
    /// Direct value-space roots for published classes, independent of lexical tickets.
    pub(in crate::check::checker) class_value_bindings:
        LayeredMap<ValueStorageId, PublishedClassValueBinding>,
    /// Const aliases that retain a standalone namespace root's completeness provenance.
    pub(in crate::check::checker) standalone_namespace_value_aliases:
        LayeredMap<ValueStorageId, ValueStorageId>,
    /// Display name by stable [`ClassId`].
    /// Lets constructor-access diagnostics name the declaring class, which may be
    /// an inherited base, while keeping [`ClassInfo`] `Copy`.
    pub(in crate::check::checker) class_names: LayeredMap<ClassId, String>,
    pub(in crate::check::checker) decl_types: DeclTypes,
    /// Construction and single-publication state for admitted function/namespace groups.
    pub(in crate::check::checker) function_groups: FunctionGroupRegistry<Ticket>,
    /// Earlier declaration-group participants that follow later groups in overload precedence.
    pub(in crate::check::checker) function_group_precedence_tails_by_name:
        FxHashMap<String, ValueStorageId>,
    /// Published function-group names inherited without construction drafts or tickets.
    pub(in crate::check::checker) named_function_symbols: LayeredSet<SymbolId>,
    /// Immutable namespace value surfaces awaiting their exact class-owned draft.
    pub(in crate::check::checker) class_namespace_payloads:
        BTreeMap<TypeGroupId, Vec<ClassNamespacePropertyPayload<Ticket>>>,
    /// Prepared attached-namespace members consumed exactly once at their source sites.
    pub(in crate::check::checker) namespace_values: NamespaceValueRegistry<Ticket>,
    /// Explicit `var` annotations reserved across one function/module hoist
    /// container, keyed by their own `(module, declarator span)` source site.
    pub(in crate::check::checker) var_annotation_surfaces:
        FxHashMap<(ScopeId, u32), VarAnnotationSurface>,
    /// Publication state for each shared `var` value declaration. This keeps a
    /// forward annotation provisional without overwriting a parameter type.
    pub(in crate::check::checker) var_value_type_states:
        FxHashMap<ValueStorageId, VarValueTypeState>,
    /// Current `this` type while checking class members.
    /// Save/restored at member boundaries so it never leaks; nested functions keep
    /// the enclosing value in this subset.
    pub(in crate::check::checker) current_this: Option<TypeId>,
    /// Non-query capability for direct `this.member` lookup in a poisoned class body.
    /// The object itself is never returned as an expression type or semantic operand.
    pub(in crate::check::checker) current_body_this_environment: Option<BodyMemberEnvironment>,
    /// Current class context for access-control checks.
    /// Save/restored with `current_this`; `private` requires the declaring class and
    /// `protected` allows subclasses. Outside class members it is `None`.
    pub(in crate::check::checker) current_class: Option<ClassId>,
    /// Lexically enclosing class contexts, outermost to innermost. Nested classes
    /// get their own `current_class` while retaining outer visibility privileges.
    pub(in crate::check::checker) enclosing_classes: Vec<ClassId>,
    /// Current base-constructor signature for checking `super(args)`.
    /// Save/restored at class-member boundaries so it never leaks; outside a
    /// derived class member, `super(...)` has no signature and is ignored.
    pub(in crate::check::checker) current_super_ctor: Option<TypeId>,
    /// Whether the current body is the declaring class's constructor.
    /// This gates the only allowed write to `readonly this.prop`: the current class
    /// must match the property's declaring class. Restored at constructor boundary.
    pub(in crate::check::checker) current_in_ctor: bool,
    /// Flow-node arena: the single narrowing model.
    /// A pre-pass lowers module/function bodies here; narrowed reference types are
    /// memoized backward walks from their flow node.
    pub(in crate::check::checker) flow_nodes: Vec<FlowNode>,
    /// The flow pre-pass's working cursor: the flow node currently in effect as the
    /// builder walks. Meaningless during the check walk (which resolves via
    /// [`reference_flow`](Pass::reference_flow)).
    pub(in crate::check::checker) flow_cursor: FlowNodeId,
    /// The flow pre-pass's enclosing-loop stack, so a `continue` can find its target
    /// loop label (the back-edge target). `break` uses [`break_targets`] instead — a
    /// `break` exits the nearest loop **or** `switch`, `continue` only a loop.
    pub(in crate::check::checker) flow_loops: Vec<FlowLoopFrame>,
    /// The flow pre-pass's enclosing-**breakable** stack (loops + switches): each
    /// entry collects the `break` edges of one construct, joined into its exit flow.
    /// Separate from [`flow_loops`] because a `break` targets the nearest loop or
    /// `switch`, while a `continue` skips any intervening `switch` to the loop label.
    pub(in crate::check::checker) break_targets: Vec<Vec<FlowNodeId>>,
    /// The flow pre-pass's named label stack. Labeled `break` edges exit the matching
    /// label's statement; labeled `continue` uses the matching labeled loop's target.
    pub(in crate::check::checker) label_targets: Vec<FlowLabelFrame>,
    /// Reference-to-flow-node map, keyed by `(module scope, reference span start)`.
    /// Misses default to START, the sound over-report. Resolver-side `SymbolId`
    /// checks keep narrowing from crossing symbols, properties, or shadowed bindings.
    pub(in crate::check::checker) reference_flow: FxHashMap<(ScopeId, u32), FlowNodeId>,
    /// Resolver memo, keyed `(flow node, symbol) → narrowed type`. Durable across
    /// the whole check pass (ids are globally unique). A value that depended on an
    /// in-progress loop back edge is **never** written here (gated on
    /// [`flow_loop_depth`](Pass::flow_loop_depth) — invariants §1).
    pub(in crate::check::checker) flow_memo: FxHashMap<(FlowNodeId, SymbolId), TypeId>,
    /// Provisional loop-label seeds during a fixpoint resolution: a re-entrant walk
    /// of a loop label returns its seed here instead of looping. Cleared per label
    /// once its fixpoint resolves; never promoted to [`flow_memo`](Pass::flow_memo).
    pub(in crate::check::checker) flow_provisional: FxHashMap<(FlowNodeId, SymbolId), TypeId>,
    /// Depth of in-progress loop-label fixpoints. `> 0` suppresses durable memo
    /// writes (the resolved value may depend on a provisional seed), which is what
    /// keeps a stale pre-loop narrow state from being cached across a back edge.
    pub(in crate::check::checker) flow_loop_depth: u32,
    /// Conditional-type lowering contexts.
    /// Frames cover the whole node, but `infer` binders are active only in extends/true.
    /// Cross-binder nested-`infer` references poison the intervening nodes; names in no
    /// active frame fall through to ordinary resolution.
    pub(in crate::check::checker) cond_frames: Vec<CondFrame>,
    /// Whether a type-declaration template is being lowered.
    /// While true, concrete conditionals stay interned templates until a value-position
    /// demand evaluates them.
    pub(in crate::check::checker) building_template: bool,
    /// The **conditional-alias declaration currently being resolved** (M25): its type
    /// storage id, name-declaration span, and name. Set while a `type A = C extends E ? …`
    /// body is lowered, so a check type that surface-references `A` itself is caught as
    /// `TK2456` at the alias declaration. `None` outside such a body.
    pub(in crate::check::checker) resolving_conditional_alias: Option<(TypeGroupId, Span, String)>,
    /// Plain alias currently being resolved, for mapped self-reference diagnostics.
    /// `lower_mapped_type` uses this to report `TK2456` at the alias declaration
    /// instead of feeding a silent re-entry error type into mapped evaluation.
    /// Separate from conditional alias tracking so nested conditionals keep M25 behavior.
    pub(in crate::check::checker) resolving_alias: Option<(TypeGroupId, Span, String)>,
    /// Stack of aliases currently resolving, used to report `TK2456` on every alias
    /// in a surface cycle. Each entry records its starting indirection depth: same-depth
    /// re-entry is circular; deeper re-entry came through a type constructor and is
    /// legal recursion, silently error-typed.
    pub(in crate::check::checker) resolving_alias_stack: Vec<(TypeGroupId, Span, String, u32)>,
    /// Current legal-recursion indirection depth.
    /// Incremented only across type constructors; unions/intersections/`keyof` stay
    /// surface cycles. Missed increments over-report `TK2456`, the safe direction.
    pub(in crate::check::checker) alias_indirection_depth: u32,
    /// Current syntactic nesting depth of the annotation being lowered (backlog 63k).
    /// Bounds host recursion in `lower_annotation` so a pathologically deep type literal
    /// reports `TK2589` instead of overflowing the stack. Balanced through every return.
    pub(in crate::check::checker) annotation_depth: u32,
    /// B29 — aliases confirmed to be part of a **surface cycle** (`TK2456` reported).
    /// Their resolution is forced to the error type (final, not provisional — a detected
    /// cycle is a settled verdict), so the M22 silent-downstream discipline holds.
    pub(in crate::check::checker) circular_aliases: FxHashSet<usize>,
    /// Mapped-type lowering contexts.
    /// `X[K]` using the innermost mapped key lowers to the node-scoped
    /// [`crate::types::repr::TypeTag::MappedValue`] placeholder instead of eager resolution.
    pub(in crate::check::checker) mapped_frames: Vec<MappedFrame>,
}

impl<'ast, Ticket: Copy + PartialEq> Deref for Pass<'_, 'ast, Ticket> {
    type Target = ConstructionDrafts<'ast>;

    fn deref(&self) -> &Self::Target {
        self.type_environment.drafts()
    }
}

impl<Ticket: Copy + PartialEq> DerefMut for Pass<'_, '_, Ticket> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.type_environment.drafts_mut()
    }
}

/// One enclosing-loop frame for the flow pre-pass: the loop's label (the
/// `continue`/back-edge target). `break` edges live on [`Pass::break_targets`]
/// (shared with `switch`), not here.
pub(in crate::check::checker) struct FlowLoopFrame {
    pub(in crate::check::checker) label: FlowNodeId,
}

/// One named label frame for the flow pre-pass.
pub(in crate::check::checker) struct FlowLabelFrame {
    pub(in crate::check::checker) name: String,
    pub(in crate::check::checker) breaks: Vec<FlowNodeId>,
    pub(in crate::check::checker) continue_target: Option<FlowNodeId>,
    pub(in crate::check::checker) allows_continue: bool,
}

/// One mapped-type lowering context (M26): the node's key binder name. Lives on
/// [`Pass::mapped_frames`] while the mapped type's value template is lowered, so an
/// indexed access on the key (`T[K]`) is recognized as the source-value placeholder.
pub(in crate::check::checker) struct MappedFrame {
    /// The key binder name (`K` in `{ [K in S]: V }`).
    pub(in crate::check::checker) key_name: String,
    /// Captured modifiers source for the `Pick` shape (`T[P]` on this frame's key).
    /// First capture wins; non-homomorphic mapped lowering consumes it.
    pub(in crate::check::checker) captured_source: Option<TypeId>,
}

/// One conditional-type lowering context (M25): the node's `infer` binder frame plus the
/// cross-binder poison flag (backlog 26 stopgap). Lives on [`Pass::cond_frames`] for the
/// whole `lower_conditional_type` call.
#[derive(Default)]
pub(in crate::check::checker) struct CondFrame {
    /// This node's `infer` name → de Bruijn index map. A new name takes index
    /// `binders.len()`; a repeated name reuses its index (`infer_count` is the final
    /// `binders.len()`).
    pub(in crate::check::checker) binders: FxHashMap<String, u32>,
    /// Whether this node's binders are reference-visible — `true` only while its true
    /// branch is lowered.
    pub(in crate::check::checker) active: bool,
    /// Whether fresh `infer` declarations may bind this frame — `true` only while its
    /// `extends` type is lowered.
    pub(in crate::check::checker) accepts_infer_declarations: bool,
    /// Set when a cross-binder reference poisons this node (see
    /// [`crate::types::repr::ConditionalType::poisoned`]).
    pub(in crate::check::checker) poisoned: bool,
}

#[cfg(test)]
mod tests {
    use super::DeclTypes;
    use crate::binder::declaration::ValueStorageId;
    use crate::check::checker::events::{user_record_ticket_key, EventStore, UserRecordTicket};
    use crate::check::checker::events_library::{LibraryEventLedger, LibraryRecordTicket};
    use crate::check::checker::lexical_events::LexicalReservations;
    use crate::check::checker::reporting_record::CheckerRecord;
    use crate::diagnostics::{Diagnostic, DiagnosticCode, IncompleteSurface};
    use crate::source::{LibraryFileOrdinal, ModuleOrdinal, SourceUnit};
    use crate::span::Span;
    use crate::types::store::TypeId;
    use crate::types::Interner;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;
    use rustc_hash::FxHashMap;

    /// The two rules the argument-walk memo's soundness rests on (backlog `95`).
    #[test]
    fn the_observation_log_separates_a_walks_inputs_from_its_outputs() {
        let mut types = DeclTypes::new(4);
        types.set(ValueStorageId(0), TypeId(10));
        types.set(ValueStorageId(1), TypeId(11));

        DeclTypes::open_log();
        let mark = DeclTypes::log_mark();

        // Read before write: an input the walk depended on.
        assert_eq!(types.get(ValueStorageId(0)), Some(TypeId(10)));
        // Written before it is read: the walk's own output, not an input.
        types.set(ValueStorageId(1), TypeId(21));
        assert_eq!(types.get(ValueStorageId(1)), Some(TypeId(21)));
        // Never observed at all.
        types.set(ValueStorageId(2), TypeId(22));

        let dependencies = DeclTypes::dependencies_since(mark);
        assert_eq!(dependencies, [(ValueStorageId(0), Some(TypeId(10)))]);
        let writes: Vec<(ValueStorageId, Option<TypeId>)> = DeclTypes::net_writes_since(mark)
            .into_iter()
            .map(|write| (write.id, write.value))
            .collect();
        assert_eq!(
            writes,
            [
                (ValueStorageId(1), Some(TypeId(21))),
                (ValueStorageId(2), Some(TypeId(22))),
            ]
        );

        // A slot written and then restored has moved nowhere, so it is not a write the
        // memo may replay — replaying it would reimpose a stale binding.
        let excursion = DeclTypes::log_mark();
        let saved = types.clone();
        types.set(ValueStorageId(3), TypeId(33));
        types.restore(saved, excursion);
        assert_eq!(types.get(ValueStorageId(3)), None);
        assert!(DeclTypes::net_writes_since(excursion).is_empty());

        DeclTypes::close_log();
        // Nothing is recorded outside an open log.
        assert_eq!(DeclTypes::log_mark(), 0);
        assert_eq!(types.get(ValueStorageId(0)), Some(TypeId(10)));
        assert!(DeclTypes::dependencies_since(0).is_empty());
    }

    #[test]
    fn declaration_types_share_a_frozen_prefix_and_isolate_dense_suffixes() {
        let mut base = DeclTypes::new(3);
        base.set(ValueStorageId(0), TypeId(10));
        base.set(ValueStorageId(2), TypeId(12));
        base.freeze_as_base().expect("declaration prefix seals");

        let mut first = base.fork_delta().expect("first declaration suffix");
        let second = base.fork_delta().expect("second declaration suffix");
        assert!(first.shares_base_with(&second));
        first.resize(5);
        first.set(ValueStorageId(3), TypeId(13));
        first.set(ValueStorageId(4), TypeId(14));

        assert_eq!(first.local_len(), 2);
        assert_eq!(first.get(ValueStorageId(0)), Some(TypeId(10)));
        assert_eq!(first.get(ValueStorageId(3)), Some(TypeId(13)));
        assert_eq!(first.get(ValueStorageId(4)), Some(TypeId(14)));
        assert_eq!(second.len(), 3);
        assert_eq!(second.get(ValueStorageId(3)), None);
        assert_eq!(base.len(), 3);
    }

    #[test]
    fn sparse_prefix_reads_invalidate_memos_after_an_override() {
        let mut base = DeclTypes::new(1);
        base.set(ValueStorageId(0), TypeId(10));
        base.freeze_as_base().expect("declaration prefix seals");
        let mut sparse = base
            .fork_sparse_delta()
            .expect("sparse declaration epoch forks");

        DeclTypes::open_log();
        let mark = DeclTypes::log_mark();
        assert_eq!(sparse.get(ValueStorageId(0)), Some(TypeId(10)));
        let dependencies = DeclTypes::dependencies_since(mark);
        assert_eq!(dependencies, [(ValueStorageId(0), Some(TypeId(10)))]);
        assert!(sparse.dependencies_hold(&dependencies));

        sparse.set(ValueStorageId(0), TypeId(20));
        assert!(!sparse.dependencies_hold(&dependencies));
        assert_eq!(sparse.get(ValueStorageId(0)), Some(TypeId(20)));
        let writes = DeclTypes::net_writes_since(mark);
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].id, ValueStorageId(0));
        assert_eq!(writes[0].previous, Some(TypeId(10)));
        assert_eq!(writes[0].value, Some(TypeId(20)));

        sparse.set(ValueStorageId(0), TypeId(10));
        assert!(DeclTypes::net_writes_since(mark).is_empty());
        DeclTypes::close_log();

        let sibling = base
            .fork_sparse_delta()
            .expect("sibling declaration epoch forks");
        assert_eq!(sibling.get(ValueStorageId(0)), Some(TypeId(10)));
    }

    #[test]
    fn library_pass_finishes_ordered_record_batches_without_reporting_authority() {
        let mut ledger = LibraryEventLedger::default();
        let first = ledger.reserve_event(LibraryFileOrdinal::new(7), 11);
        let first_owner: LibraryRecordTicket = first.primary;
        let nested_owner = ledger
            .reserve_record(first.id)
            .expect("nested record reservation");
        let second_owner: LibraryRecordTicket =
            ledger.reserve_event(LibraryFileOrdinal::new(7), 13).primary;

        let prelude_allocator = Allocator::default();
        let user_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
        let user = Parser::new(&user_allocator, "", SourceType::ts()).parse();
        let binder = crate::binder::bind_module_with_prelude(&prelude.program, &user.program);
        let mut interner = Interner::with_intrinsics();
        let string = interner.well_known().string;
        let number = interner.well_known().number;
        let mut pass = super::super::build_pass_with_tickets(
            &mut interner,
            &binder,
            Vec::new().into(),
            vec![None; binder.type_groups.len()].into(),
            super::DeclTypes::new(binder.decl_count),
            0,
            super::super::PassReportingPlan {
                reporting: super::super::PassReporting {
                    source: SourceUnit::Library {
                        file_ordinal: LibraryFileOrdinal::new(7),
                    },
                    lexical_events: LexicalReservations::default(),
                    suppress_effects: false,
                },
                pending_tickets: vec![first_owner, second_owner],
                ticket_key: crate::check::checker::events_library::library_record_ticket_key,
            },
        );
        pass.type_environment = super::super::type_groups::TypeEnvironmentState::Published(
            super::super::type_groups::PublishedTypeEnvironment::empty(),
        );
        assert_eq!(
            pass.current_source,
            SourceUnit::Library {
                file_ordinal: LibraryFileOrdinal::new(7),
            }
        );
        pass.pending_effects[0]
            .records
            .diagnostic(Diagnostic::cannot_find_name(Span::new(11, 12), "first"));
        pass.pending_effects[0]
            .records
            .incomplete(IncompleteSurface::new(
                "library/effects",
                Span::new(12, 13),
                "second",
            ));
        pass.pending_effects[0].push_constraint_check(
            super::ConstraintCheckObligation {
                checks: vec![(Some(string), number, Span::new(14, 15))],
                substitutions: FxHashMap::default(),
            },
            None,
        );
        pass.pending_effects[1]
            .records
            .record(CheckerRecord::Diagnostic(Diagnostic::cannot_find_name(
                Span::new(13, 14),
                "third",
            )));
        pass.with_ticket_effects(second_owner, |pass| {
            pass.emit_diagnostic(Diagnostic::cannot_find_name(Span::new(15, 16), "fourth"));
        });
        pass.with_ticket_effects(first_owner, |pass| {
            pass.emit_diagnostic(Diagnostic::cannot_find_name(Span::new(16, 17), "fifth"));
            pass.with_ticket_effects(nested_owner, |pass| {
                pass.emit_diagnostic(Diagnostic::cannot_find_name(Span::new(17, 18), "nested"));
            });
        });
        pass.with_ticket_effects(second_owner, |pass| {
            pass.emit_diagnostic(Diagnostic::cannot_find_name(Span::new(18, 19), "sixth"));
        });
        let batches = super::super::finish_semantic_effects(&mut pass);

        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].owner(), first_owner);
        assert_eq!(batches[1].owner(), second_owner);
        assert_eq!(batches[2].owner(), nested_owner);
        assert!(matches!(
            batches[0].records(),
            [
                CheckerRecord::Diagnostic(first),
                CheckerRecord::Incomplete(second),
                CheckerRecord::Diagnostic(fifth),
                CheckerRecord::Diagnostic(constraint),
            ] if first.span.start == 11
                && second.span.start == 12
                && fifth.span.start == 16
                && constraint.code == DiagnosticCode::TK2344
                && constraint.span == Span::new(14, 15)
        ));
        assert!(matches!(
            batches[1].records(),
            [
                CheckerRecord::Diagnostic(third),
                CheckerRecord::Diagnostic(fourth),
                CheckerRecord::Diagnostic(sixth),
            ] if third.span.start == 13 && fourth.span.start == 15 && sixth.span.start == 18
        ));
        assert!(matches!(
            batches[2].records(),
            [CheckerRecord::Diagnostic(nested)] if nested.span.start == 17
        ));

        for batch in batches {
            let (owner, records) = batch.into_parts();
            ledger.complete(owner, records).expect("complete batch");
        }
        let replay = ledger.finish().expect("complete library replay");
        assert_eq!(
            replay
                .iter()
                .map(|(key, _)| (key.event_ordinal, key.record_ordinal))
                .collect::<Vec<_>>(),
            vec![
                (0, 0),
                (0, 0),
                (0, 0),
                (0, 0),
                (0, 1),
                (1, 0),
                (1, 0),
                (1, 0)
            ]
        );
    }

    #[test]
    fn user_effect_index_coalesces_out_of_order_and_nested_owners_without_reordering_replay() {
        let mut store = EventStore::default();
        let first = store.reserve_event(ModuleOrdinal::new(0), 11);
        let nested_owner = store
            .reserve_record(first.id)
            .expect("nested record reservation");
        let second_owner = store.reserve_event(ModuleOrdinal::new(0), 13).primary;
        let first_owner: UserRecordTicket = first.primary;

        let prelude_allocator = Allocator::default();
        let user_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
        let user = Parser::new(&user_allocator, "", SourceType::ts()).parse();
        let binder = crate::binder::bind_module_with_prelude(&prelude.program, &user.program);
        let mut interner = Interner::with_intrinsics();
        let mut pass = super::super::build_pass_with_tickets(
            &mut interner,
            &binder,
            Vec::new().into(),
            vec![None; binder.type_groups.len()].into(),
            super::DeclTypes::new(binder.decl_count),
            0,
            super::super::PassReportingPlan {
                reporting: super::super::PassReporting {
                    source: SourceUnit::User {
                        module_ordinal: ModuleOrdinal::new(0),
                        unit_slot: crate::source::UnitSlot::new(0),
                    },
                    lexical_events: LexicalReservations::default(),
                    suppress_effects: false,
                },
                pending_tickets: vec![first_owner, second_owner],
                ticket_key: user_record_ticket_key,
            },
        );
        pass.type_environment = super::super::type_groups::TypeEnvironmentState::Published(
            super::super::type_groups::PublishedTypeEnvironment::empty(),
        );

        pass.with_ticket_effects(second_owner, |pass| {
            pass.emit_diagnostic(Diagnostic::cannot_find_name(Span::new(13, 14), "second"));
        });
        pass.with_ticket_effects(first_owner, |pass| {
            pass.emit_diagnostic(Diagnostic::cannot_find_name(Span::new(11, 12), "first"));
            pass.with_ticket_effects(nested_owner, |pass| {
                pass.emit_diagnostic(Diagnostic::cannot_find_name(Span::new(12, 13), "nested"));
            });
        });
        pass.with_ticket_effects(second_owner, |pass| {
            pass.emit_diagnostic(Diagnostic::cannot_find_name(
                Span::new(14, 15),
                "second-again",
            ));
        });

        let batches = super::super::finish_semantic_effects(&mut pass);
        assert_eq!(
            batches
                .iter()
                .map(|batch| batch.owner())
                .collect::<Vec<_>>(),
            vec![first_owner, second_owner, nested_owner]
        );
        for batch in batches {
            let (owner, records) = batch.into_parts();
            store.complete(owner, records).expect("complete batch");
        }
        let replay = store.finish().expect("complete user replay");
        assert_eq!(
            replay
                .iter()
                .map(|(key, _)| (key.event_ordinal, key.record_ordinal))
                .collect::<Vec<_>>(),
            vec![(0, 0), (0, 1), (1, 0), (1, 0)]
        );
    }
}

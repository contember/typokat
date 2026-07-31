//! Single-publication state for admitted function/namespace value merges.

use super::events::UserRecordTicket;
use crate::binder::declaration::ValueStorageId;
use crate::binder::namespace::NamespaceValueAttachmentDisposition;
use crate::binder::scope::ScopeId;
use crate::binder::symbol::SymbolId;
use crate::binder::Binder;
use crate::types::repr::PropertyType;
use crate::types::store::TypeId;
use rustc_hash::FxHashMap;

/// Binder-owned identity of one admitted lexical function/namespace merge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct FunctionGroupIdentity {
    pub(in crate::check::checker) symbol: SymbolId,
    pub(in crate::check::checker) name: String,
    pub(in crate::check::checker) participants: Vec<ValueStorageId>,
    namespace_payload_required: bool,
}

/// Namespace-side input frozen before a merged function can be published.
#[derive(Clone, Debug)]
pub(in crate::check::checker) enum FunctionNamespacePayload<Ticket: Copy = UserRecordTicket> {
    Ready(Vec<PropertyType>),
    Unavailable { owner: Option<Ticket> },
}

/// Why an inferred merged function can no longer publish a callable value.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum FunctionGroupUnavailableCause {
    Signature,
    NamespacePayload,
    InferredReturnCycle,
    InferredReturnDependency,
}

/// Result of demanding a symbol before going through flow resolution.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum FunctionGroupDemand {
    NotGroup,
    Ready(TypeId),
    /// A source use owns an incomplete record; an inference dependency does not.
    Pending {
        report_use: bool,
    },
    /// Construction-private callable used only for exact unconditional recursion.
    PrivateSelf(TypeId),
    Unavailable,
}

/// Ingredients for the one immutable object publication.
#[derive(Clone, Debug)]
pub(in crate::check::checker) struct FunctionGroupPublication {
    pub(in crate::check::checker) symbol: SymbolId,
    pub(in crate::check::checker) participants: Vec<ValueStorageId>,
    pub(in crate::check::checker) properties: Vec<PropertyType>,
    pub(in crate::check::checker) call_signatures: Vec<TypeId>,
    pub(in crate::check::checker) prepublished_participants: Vec<ValueStorageId>,
}

/// Terminal result of one unannotated ordinary function body.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum FunctionGroupBodyCompletion<Ticket: Copy = UserRecordTicket> {
    Ready,
    Unavailable {
        cause: FunctionGroupUnavailableCause,
        owner: Option<Ticket>,
    },
}

#[derive(Clone, Debug)]
enum NamespacePayloadState {
    Missing,
    Ready(Vec<PropertyType>),
    Unavailable,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum FunctionGroupState<Ticket: Copy> {
    Building,
    Published {
        ty: TypeId,
    },
    Unavailable {
        cause: FunctionGroupUnavailableCause,
        owner: Option<Ticket>,
    },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum FunctionParticipantState {
    Unseen,
    Public(TypeId),
    ValidationOnly,
    WaitingForBody,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct FunctionParticipantSlot {
    declaration: ValueStorageId,
    state: FunctionParticipantState,
}

/// The sole mutable construction object for one admitted merge.
#[derive(Clone, Debug)]
struct FunctionGroupDraft<Ticket: Copy> {
    symbol: SymbolId,
    name: String,
    participants: Vec<FunctionParticipantSlot>,
    inherited_call_signatures: Vec<TypeId>,
    namespace_payload: NamespacePayloadState,
    state: FunctionGroupState<Ticket>,
}

#[derive(Copy, Clone, Debug)]
struct ActiveFunctionGroupFill<Ticket: Copy> {
    symbol: SymbolId,
    owner: Option<Ticket>,
    private_self: Option<TypeId>,
    dependency: Option<SymbolId>,
    dependency_unavailable: bool,
}

/// Checker-wide registry. Drafts never escape this construction-only owner.
pub(in crate::check::checker) struct FunctionGroupRegistry<Ticket: Copy = UserRecordTicket> {
    groups: FxHashMap<SymbolId, FunctionGroupDraft<Ticket>>,
    by_value: FxHashMap<ValueStorageId, SymbolId>,
    active_fills: Vec<ActiveFunctionGroupFill<Ticket>>,
}

impl<Ticket: Copy> Default for FunctionGroupRegistry<Ticket> {
    fn default() -> Self {
        Self {
            groups: FxHashMap::default(),
            by_value: FxHashMap::default(),
            active_fills: Vec::new(),
        }
    }
}

impl<Ticket: Copy> FunctionGroupRegistry<Ticket> {
    /// Resolve only a binder-approved function owner with an attachable namespace
    /// value surface.
    pub(in crate::check::checker) fn function_namespace_identity(
        binder: &Binder,
        scope: ScopeId,
        name: &str,
    ) -> Option<FunctionGroupIdentity> {
        let attachment = binder.namespace_value_attachment(scope, name);
        let (symbol, namespace_payload_required) = match attachment {
            Some(attachment) => {
                match attachment.disposition {
                    NamespaceValueAttachmentDisposition::AdmittedFunction => {}
                    NamespaceValueAttachmentDisposition::DeferredFunctionBacklog42 => {}
                    NamespaceValueAttachmentDisposition::AdmittedClass
                    | NamespaceValueAttachmentDisposition::TypeContainerOnly
                    | NamespaceValueAttachmentDisposition::Rejected(_) => return None,
                }
                (attachment.symbol, true)
            }
            None => (binder.graph.resolve(scope, name)?, false),
        };
        let binding = binder.symbols.get(symbol)?;
        if binding.function_values.len() < 2 && !namespace_payload_required {
            return None;
        }
        Some(FunctionGroupIdentity {
            symbol,
            name: name.to_owned(),
            participants: binding.function_values.clone(),
            namespace_payload_required,
        })
    }

    pub(in crate::check::checker) fn register(&mut self, identity: FunctionGroupIdentity) {
        if let Some(existing) = self.groups.get(&identity.symbol) {
            assert_eq!(existing.name, identity.name, "function group name changed");
            assert_eq!(
                existing
                    .participants
                    .iter()
                    .map(|participant| participant.declaration)
                    .collect::<Vec<_>>(),
                identity.participants,
                "function group participants changed"
            );
            return;
        }
        assert!(
            !identity.participants.is_empty(),
            "function group requires a callable participant"
        );
        for declaration in &identity.participants {
            let previous = self.by_value.insert(*declaration, identity.symbol);
            assert!(
                previous.is_none(),
                "one value declaration cannot participate in two function groups"
            );
        }
        self.groups.insert(
            identity.symbol,
            FunctionGroupDraft {
                symbol: identity.symbol,
                name: identity.name,
                participants: identity
                    .participants
                    .into_iter()
                    .map(|declaration| FunctionParticipantSlot {
                        declaration,
                        state: FunctionParticipantState::Unseen,
                    })
                    .collect(),
                inherited_call_signatures: Vec::new(),
                namespace_payload: if identity.namespace_payload_required {
                    NamespacePayloadState::Missing
                } else {
                    NamespacePayloadState::Ready(Vec::new())
                },
                state: FunctionGroupState::Building,
            },
        );
    }

    pub(in crate::check::checker) fn seed_inherited_publication(
        &mut self,
        symbol: SymbolId,
        participants: &[ValueStorageId],
        call_signatures: Vec<TypeId>,
    ) {
        let Some(draft) = self.groups.get_mut(&symbol) else {
            return;
        };
        if !matches!(draft.state, FunctionGroupState::Building) || call_signatures.is_empty() {
            return;
        }
        if !draft.inherited_call_signatures.is_empty() {
            return;
        }
        draft.inherited_call_signatures = call_signatures;
        for declaration in participants {
            if let Some(participant) = draft
                .participants
                .iter_mut()
                .find(|participant| participant.declaration == *declaration)
            {
                if matches!(participant.state, FunctionParticipantState::Unseen) {
                    participant.state = FunctionParticipantState::ValidationOnly;
                }
            }
        }
    }

    pub(in crate::check::checker) fn install_namespace_payload(
        &mut self,
        symbol: SymbolId,
        payload: FunctionNamespacePayload<Ticket>,
    ) {
        let draft = self
            .groups
            .get_mut(&symbol)
            .expect("namespace payload requires a registered function group");
        assert!(
            matches!(draft.namespace_payload, NamespacePayloadState::Missing),
            "function group namespace payload installed twice"
        );
        match payload {
            FunctionNamespacePayload::Ready(properties) => {
                draft.namespace_payload = NamespacePayloadState::Ready(properties);
            }
            FunctionNamespacePayload::Unavailable { owner } => {
                draft.namespace_payload = NamespacePayloadState::Unavailable;
                draft.state = FunctionGroupState::Unavailable {
                    cause: FunctionGroupUnavailableCause::NamespacePayload,
                    owner,
                };
            }
        }
    }

    pub(in crate::check::checker) fn namespace_payload_for_value(
        &self,
        value: ValueStorageId,
    ) -> Option<&[PropertyType]> {
        let symbol = self.by_value.get(&value)?;
        let draft = self.groups.get(symbol)?;
        match &draft.namespace_payload {
            NamespacePayloadState::Ready(properties) => Some(properties),
            NamespacePayloadState::Missing | NamespacePayloadState::Unavailable => None,
        }
    }

    /// Add one externally visible row. Overload implementations never call this.
    pub(in crate::check::checker) fn reserve_public_row(
        &mut self,
        symbol: SymbolId,
        declaration: ValueStorageId,
        ty: TypeId,
    ) {
        let Some(draft) = self.groups.get_mut(&symbol) else {
            return;
        };
        if !matches!(draft.state, FunctionGroupState::Building) {
            return;
        }
        let Some(participant) = draft
            .participants
            .iter_mut()
            .find(|participant| participant.declaration == declaration)
        else {
            return;
        };
        match participant.state {
            FunctionParticipantState::Unseen => {
                participant.state = FunctionParticipantState::Public(ty);
            }
            FunctionParticipantState::Public(existing) if existing == ty => {}
            FunctionParticipantState::Public(_)
            | FunctionParticipantState::ValidationOnly
            | FunctionParticipantState::WaitingForBody => {}
        }
    }

    /// Mark an overload implementation terminal without exposing its signature.
    pub(in crate::check::checker) fn reserve_validation_only(
        &mut self,
        symbol: SymbolId,
        declaration: ValueStorageId,
    ) {
        let Some(draft) = self.groups.get_mut(&symbol) else {
            return;
        };
        if !matches!(draft.state, FunctionGroupState::Building) {
            return;
        }
        let Some(participant) = draft
            .participants
            .iter_mut()
            .find(|participant| participant.declaration == declaration)
        else {
            return;
        };
        if matches!(participant.state, FunctionParticipantState::Unseen) {
            participant.state = FunctionParticipantState::ValidationOnly;
        }
    }

    pub(in crate::check::checker) fn wait_for_body(
        &mut self,
        symbol: SymbolId,
        declaration: ValueStorageId,
    ) {
        let Some(draft) = self.groups.get_mut(&symbol) else {
            return;
        };
        if !matches!(draft.state, FunctionGroupState::Building) {
            return;
        }
        let Some(participant) = draft
            .participants
            .iter_mut()
            .find(|participant| participant.declaration == declaration)
        else {
            return;
        };
        if matches!(participant.state, FunctionParticipantState::Unseen) {
            participant.state = FunctionParticipantState::WaitingForBody;
        }
    }

    pub(in crate::check::checker) fn begin_body(
        &mut self,
        symbol: SymbolId,
        declaration: ValueStorageId,
        owner: Option<Ticket>,
        private_self: Option<TypeId>,
    ) {
        let draft = self
            .groups
            .get(&symbol)
            .expect("body fill requires a registered function group");
        assert!(matches!(draft.state, FunctionGroupState::Building));
        assert!(draft.participants.iter().any(|participant| {
            participant.declaration == declaration
                && participant.state == FunctionParticipantState::WaitingForBody
        }));
        self.active_fills.push(ActiveFunctionGroupFill {
            symbol,
            owner,
            private_self,
            dependency: None,
            dependency_unavailable: false,
        });
    }

    pub(in crate::check::checker) fn finish_body(
        &mut self,
        symbol: SymbolId,
        declaration: ValueStorageId,
        completed_ty: TypeId,
    ) -> FunctionGroupBodyCompletion<Ticket> {
        let active = self
            .active_fills
            .pop()
            .expect("function group body completion requires an active fill");
        assert_eq!(active.symbol, symbol, "function group fills must be nested");
        let draft = self
            .groups
            .get_mut(&symbol)
            .expect("body completion requires a registered function group");
        assert!(matches!(draft.state, FunctionGroupState::Building));
        let participant = draft
            .participants
            .iter_mut()
            .find(|participant| participant.declaration == declaration)
            .expect("body completion requires a binder-known participant");
        assert_eq!(
            participant.state,
            FunctionParticipantState::WaitingForBody,
            "body completion does not match the waiting participant"
        );
        if let Some(dependency) = active.dependency {
            let cause = if dependency == symbol {
                FunctionGroupUnavailableCause::InferredReturnCycle
            } else {
                FunctionGroupUnavailableCause::InferredReturnDependency
            };
            draft.state = FunctionGroupState::Unavailable {
                cause,
                owner: active.owner,
            };
            return FunctionGroupBodyCompletion::Unavailable {
                cause,
                owner: active.owner,
            };
        }
        if active.dependency_unavailable {
            let cause = FunctionGroupUnavailableCause::InferredReturnDependency;
            draft.state = FunctionGroupState::Unavailable {
                cause,
                owner: active.owner,
            };
            return FunctionGroupBodyCompletion::Unavailable {
                cause,
                owner: active.owner,
            };
        }
        participant.state = FunctionParticipantState::Public(completed_ty);
        FunctionGroupBodyCompletion::Ready
    }

    pub(in crate::check::checker) fn mark_unavailable(
        &mut self,
        symbol: SymbolId,
        cause: FunctionGroupUnavailableCause,
        owner: Option<Ticket>,
    ) {
        let draft = self
            .groups
            .get_mut(&symbol)
            .expect("unavailable function group must be registered");
        if matches!(draft.state, FunctionGroupState::Published { .. }) {
            return;
        }
        draft.state = FunctionGroupState::Unavailable { cause, owner };
    }

    pub(in crate::check::checker) fn demand(&mut self, symbol: SymbolId) -> FunctionGroupDemand {
        let Some(state) = self.groups.get(&symbol).map(|draft| draft.state) else {
            return FunctionGroupDemand::NotGroup;
        };
        match state {
            FunctionGroupState::Published { ty } => FunctionGroupDemand::Ready(ty),
            FunctionGroupState::Unavailable { .. } => {
                if let Some(active) = self.active_fills.last_mut() {
                    active.dependency_unavailable = true;
                }
                FunctionGroupDemand::Unavailable
            }
            FunctionGroupState::Building => {
                if let Some(active) = self.active_fills.last_mut() {
                    if active.symbol == symbol {
                        if let Some(private_self) = active.private_self {
                            return FunctionGroupDemand::PrivateSelf(private_self);
                        }
                    }
                    active.dependency.get_or_insert(symbol);
                    FunctionGroupDemand::Pending { report_use: false }
                } else {
                    FunctionGroupDemand::Pending { report_use: true }
                }
            }
        }
    }

    pub(in crate::check::checker) fn publication_plan(
        &self,
        symbol: SymbolId,
    ) -> Option<FunctionGroupPublication> {
        let draft = self
            .groups
            .get(&symbol)
            .expect("publication requires a registered function group");
        if !matches!(draft.state, FunctionGroupState::Building) {
            return None;
        }
        let NamespacePayloadState::Ready(properties) = &draft.namespace_payload else {
            return None;
        };
        if draft.participants.iter().any(|participant| {
            matches!(
                participant.state,
                FunctionParticipantState::Unseen | FunctionParticipantState::WaitingForBody
            )
        }) {
            return None;
        }
        let mut call_signatures = draft
            .participants
            .iter()
            .filter_map(|participant| match participant.state {
                FunctionParticipantState::Public(ty) => Some(ty),
                FunctionParticipantState::ValidationOnly
                | FunctionParticipantState::Unseen
                | FunctionParticipantState::WaitingForBody => None,
            })
            .collect::<Vec<_>>();
        call_signatures.extend(draft.inherited_call_signatures.iter().copied());
        if call_signatures.is_empty() {
            return None;
        }
        Some(FunctionGroupPublication {
            symbol: draft.symbol,
            participants: draft
                .participants
                .iter()
                .map(|participant| participant.declaration)
                .collect(),
            properties: properties.clone(),
            call_signatures,
            prepublished_participants: draft
                .participants
                .iter()
                .filter(|participant| participant.state == FunctionParticipantState::ValidationOnly)
                .map(|participant| participant.declaration)
                .collect(),
        })
    }

    pub(in crate::check::checker) fn mark_published(&mut self, symbol: SymbolId, ty: TypeId) {
        let draft = self
            .groups
            .get_mut(&symbol)
            .expect("publication requires a registered function group");
        if !matches!(draft.state, FunctionGroupState::Building)
            || !matches!(draft.namespace_payload, NamespacePayloadState::Ready(_))
            || draft.participants.iter().any(|participant| {
                matches!(
                    participant.state,
                    FunctionParticipantState::Unseen | FunctionParticipantState::WaitingForBody
                )
            })
            || (draft.inherited_call_signatures.is_empty()
                && !draft.participants.iter().any(|participant| {
                    matches!(participant.state, FunctionParticipantState::Public(_))
                }))
        {
            return;
        }
        draft.state = FunctionGroupState::Published { ty };
    }

    pub(in crate::check::checker) fn contains_symbol(&self, symbol: SymbolId) -> bool {
        self.groups.contains_key(&symbol)
    }

    pub(in crate::check::checker) fn frozen_symbols(&self) -> rustc_hash::FxHashSet<SymbolId> {
        self.groups.keys().copied().collect()
    }

    pub(in crate::check::checker) fn is_waiting_for(
        &self,
        symbol: SymbolId,
        declaration: ValueStorageId,
    ) -> bool {
        self.groups.get(&symbol).is_some_and(|draft| {
            matches!(draft.state, FunctionGroupState::Building)
                && draft.participants.iter().any(|participant| {
                    participant.declaration == declaration
                        && participant.state == FunctionParticipantState::WaitingForBody
                })
        })
    }

    /// Unpublished groups must be intercepted before flow can cache a recovery type.
    pub(in crate::check::checker) fn requires_demand_intercept(&self, symbol: SymbolId) -> bool {
        self.groups
            .get(&symbol)
            .is_some_and(|draft| !matches!(draft.state, FunctionGroupState::Published { .. }))
    }

    pub(in crate::check::checker) fn symbol_for_value(
        &self,
        declaration: ValueStorageId,
    ) -> Option<SymbolId> {
        self.by_value.get(&declaration).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> FunctionGroupRegistry<UserRecordTicket> {
        FunctionGroupRegistry::default()
    }

    fn identity() -> FunctionGroupIdentity {
        FunctionGroupIdentity {
            symbol: SymbolId(3),
            name: "Merged".to_owned(),
            participants: vec![ValueStorageId(4), ValueStorageId(7)],
            namespace_payload_required: true,
        }
    }

    #[test]
    fn split_public_rows_gate_publication_and_keep_binder_order() {
        let mut registry = registry();
        registry.register(identity());
        registry
            .install_namespace_payload(SymbolId(3), FunctionNamespacePayload::Ready(Vec::new()));
        registry.reserve_public_row(SymbolId(3), ValueStorageId(7), TypeId(20));
        assert!(registry.publication_plan(SymbolId(3)).is_none());

        registry.reserve_public_row(SymbolId(3), ValueStorageId(4), TypeId(10));

        let plan = registry
            .publication_plan(SymbolId(3))
            .expect("ready group publishes");
        assert_eq!(plan.call_signatures, vec![TypeId(10), TypeId(20)]);
        assert_eq!(
            plan.participants,
            vec![ValueStorageId(4), ValueStorageId(7)]
        );
    }

    #[test]
    fn validation_only_participant_is_terminal_but_not_public() {
        let mut registry = registry();
        registry.register(identity());
        registry
            .install_namespace_payload(SymbolId(3), FunctionNamespacePayload::Ready(Vec::new()));
        registry.reserve_public_row(SymbolId(3), ValueStorageId(4), TypeId(10));
        assert!(registry.publication_plan(SymbolId(3)).is_none());

        registry.reserve_validation_only(SymbolId(3), ValueStorageId(7));
        let plan = registry
            .publication_plan(SymbolId(3))
            .expect("validation-only implementation completes reservation");
        assert_eq!(plan.call_signatures, vec![TypeId(10)]);
        assert_eq!(
            plan.participants,
            vec![ValueStorageId(4), ValueStorageId(7)]
        );
    }

    #[test]
    fn namespace_payload_is_an_independent_publication_gate() {
        let mut registry = registry();
        registry.register(identity());
        registry.reserve_public_row(SymbolId(3), ValueStorageId(4), TypeId(10));
        registry.reserve_public_row(SymbolId(3), ValueStorageId(7), TypeId(20));
        assert!(registry.publication_plan(SymbolId(3)).is_none());

        registry
            .install_namespace_payload(SymbolId(3), FunctionNamespacePayload::Ready(Vec::new()));
        assert!(registry.publication_plan(SymbolId(3)).is_some());
    }

    #[test]
    fn source_demand_is_pending_without_changing_waiting_state() {
        let mut registry = registry();
        registry.register(identity());
        registry.wait_for_body(SymbolId(3), ValueStorageId(4));

        assert_eq!(
            registry.demand(SymbolId(3)),
            FunctionGroupDemand::Pending { report_use: true }
        );
        assert!(registry.publication_plan(SymbolId(3)).is_none());
    }

    #[test]
    fn waiting_body_blocks_split_publication_until_completion() {
        let mut registry = registry();
        registry.register(identity());
        registry
            .install_namespace_payload(SymbolId(3), FunctionNamespacePayload::Ready(Vec::new()));
        registry.reserve_public_row(SymbolId(3), ValueStorageId(4), TypeId(10));
        registry.wait_for_body(SymbolId(3), ValueStorageId(7));
        assert!(registry.publication_plan(SymbolId(3)).is_none());

        registry.begin_body(SymbolId(3), ValueStorageId(7), None, None);
        assert_eq!(
            registry.finish_body(SymbolId(3), ValueStorageId(7), TypeId(20)),
            FunctionGroupBodyCompletion::Ready
        );
        let plan = registry
            .publication_plan(SymbolId(3))
            .expect("completed body makes every participant terminal");
        assert_eq!(plan.call_signatures, vec![TypeId(10), TypeId(20)]);
    }

    #[test]
    fn inferred_self_dependency_is_terminally_unavailable() {
        let mut registry = registry();
        registry.register(identity());
        registry.wait_for_body(SymbolId(3), ValueStorageId(4));
        registry.begin_body(SymbolId(3), ValueStorageId(4), None, None);

        assert_eq!(
            registry.demand(SymbolId(3)),
            FunctionGroupDemand::Pending { report_use: false }
        );
        assert_eq!(
            registry.finish_body(SymbolId(3), ValueStorageId(4), TypeId(10)),
            FunctionGroupBodyCompletion::Unavailable {
                cause: FunctionGroupUnavailableCause::InferredReturnCycle,
                owner: None,
            }
        );
        assert_eq!(
            registry.demand(SymbolId(3)),
            FunctionGroupDemand::Unavailable
        );
    }

    #[test]
    fn inferred_cross_group_dependency_is_terminally_unavailable() {
        let mut registry = registry();
        registry.register(identity());
        registry.register(FunctionGroupIdentity {
            symbol: SymbolId(8),
            name: "Dependency".to_owned(),
            participants: vec![ValueStorageId(12)],
            namespace_payload_required: true,
        });
        registry.wait_for_body(SymbolId(3), ValueStorageId(4));
        registry.wait_for_body(SymbolId(8), ValueStorageId(12));
        registry.begin_body(SymbolId(3), ValueStorageId(4), None, None);

        assert_eq!(
            registry.demand(SymbolId(8)),
            FunctionGroupDemand::Pending { report_use: false }
        );
        assert_eq!(
            registry.finish_body(SymbolId(3), ValueStorageId(4), TypeId(10)),
            FunctionGroupBodyCompletion::Unavailable {
                cause: FunctionGroupUnavailableCause::InferredReturnDependency,
                owner: None,
            }
        );
    }

    #[test]
    fn private_self_callable_preserves_pure_never_recursion() {
        let mut registry = registry();
        registry.register(identity());
        registry.wait_for_body(SymbolId(3), ValueStorageId(4));
        registry.begin_body(SymbolId(3), ValueStorageId(4), None, Some(TypeId(99)));

        assert_eq!(
            registry.demand(SymbolId(3)),
            FunctionGroupDemand::PrivateSelf(TypeId(99))
        );
        assert_eq!(
            registry.finish_body(SymbolId(3), ValueStorageId(4), TypeId(100)),
            FunctionGroupBodyCompletion::Ready
        );
    }

    #[test]
    fn published_group_cannot_plan_or_transition_twice() {
        let mut registry = registry();
        registry.register(identity());
        registry
            .install_namespace_payload(SymbolId(3), FunctionNamespacePayload::Ready(Vec::new()));
        registry.reserve_public_row(SymbolId(3), ValueStorageId(4), TypeId(10));
        registry.reserve_validation_only(SymbolId(3), ValueStorageId(7));
        registry.mark_published(SymbolId(3), TypeId(30));
        assert!(registry.publication_plan(SymbolId(3)).is_none());

        registry.mark_published(SymbolId(3), TypeId(31));
        assert_eq!(
            registry.demand(SymbolId(3)),
            FunctionGroupDemand::Ready(TypeId(30))
        );
    }

    #[test]
    fn published_group_no_longer_requires_flow_interception() {
        let mut registry = registry();
        registry.register(identity());
        registry
            .install_namespace_payload(SymbolId(3), FunctionNamespacePayload::Ready(Vec::new()));
        registry.reserve_public_row(SymbolId(3), ValueStorageId(4), TypeId(10));
        registry.reserve_validation_only(SymbolId(3), ValueStorageId(7));
        assert!(registry.requires_demand_intercept(SymbolId(3)));

        registry.mark_published(SymbolId(3), TypeId(30));
        assert!(!registry.requires_demand_intercept(SymbolId(3)));
        assert_eq!(
            registry.demand(SymbolId(3)),
            FunctionGroupDemand::Ready(TypeId(30))
        );
    }

    #[test]
    fn private_replay_places_new_overloads_before_inherited_rows() {
        let mut registry = registry();
        registry.register(identity());
        registry.seed_inherited_publication(SymbolId(3), &[ValueStorageId(4)], vec![TypeId(10)]);
        registry
            .install_namespace_payload(SymbolId(3), FunctionNamespacePayload::Ready(Vec::new()));
        registry.reserve_public_row(SymbolId(3), ValueStorageId(7), TypeId(11));

        assert_eq!(
            registry
                .publication_plan(SymbolId(3))
                .map(|publication| publication.call_signatures),
            Some(vec![TypeId(11), TypeId(10)])
        );
    }

    #[test]
    fn unavailable_namespace_payload_terminally_ignores_callable_rows() {
        let mut registry = registry();
        registry.register(identity());
        registry.install_namespace_payload(
            SymbolId(3),
            FunctionNamespacePayload::Unavailable { owner: None },
        );

        registry.reserve_public_row(SymbolId(3), ValueStorageId(4), TypeId(10));
        registry.wait_for_body(SymbolId(3), ValueStorageId(4));
        assert!(registry.publication_plan(SymbolId(3)).is_none());
        assert_eq!(
            registry.demand(SymbolId(3)),
            FunctionGroupDemand::Unavailable
        );
    }
}

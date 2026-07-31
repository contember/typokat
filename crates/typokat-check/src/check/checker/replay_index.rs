//! Compiler-owned dependency evidence for selective library collision replay.

use super::classes::construction::dependency_first_sccs;
use super::events_library::LibraryEventKey;
use crate::binder::declaration::{
    DeclarationKind, SourceGlobalContributorKind, TypeGroupId, ValueStorageId,
};
use crate::binder::namespace::{DeclarationOwner, NamespaceId};
use crate::check::query::PublishedClassLookup;
use crate::class_semantics::{DemandOutcome, PublishedClassSurface, PublishedClasses};
use crate::source::LibraryFileOrdinal;
use crate::span::Span;
use crate::types::repr::ClassId;
use sha2::{Digest, Sha256};
use smallvec::SmallVec;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;

const COLLISION_REPLAY_MANIFEST_DOMAIN: &[u8] = b"typokat-collision-replay-index-v1";

/// Stable semantic publication domains. Tag order is part of the manifest wire contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReplayOwner {
    TypeGroup(TypeGroupId),
    Value(ValueStorageId),
    Namespace(NamespaceId),
    Class(ClassId),
    GlobalObject,
    Statement(LibraryEventKey),
}

impl PartialOrd for ReplayOwner {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ReplayOwner {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn payload(owner: ReplayOwner) -> (u64, u64, u64, u64) {
            match owner {
                ReplayOwner::TypeGroup(id) => (u64::from(id.0), 0, 0, 0),
                ReplayOwner::Value(id) => (u64::from(id.0), 0, 0, 0),
                ReplayOwner::Namespace(id) => (u64::from(id.0), 0, 0, 0),
                ReplayOwner::Class(id) => (u64::from(id.0), 0, 0, 0),
                ReplayOwner::GlobalObject => (0, 0, 0, 0),
                ReplayOwner::Statement(key) => (
                    u64::try_from(key.file_ordinal.index()).unwrap_or(u64::MAX),
                    u64::from(key.source_start),
                    u64::try_from(key.event_ordinal).unwrap_or(u64::MAX),
                    u64::try_from(key.record_ordinal).unwrap_or(u64::MAX),
                ),
            }
        }
        self.tag()
            .cmp(&other.tag())
            .then_with(|| payload(*self).cmp(&payload(*other)))
    }
}

impl ReplayOwner {
    pub const fn tag(self) -> u8 {
        match self {
            Self::TypeGroup(_) => 0,
            Self::Value(_) => 1,
            Self::Namespace(_) => 2,
            Self::Class(_) => 3,
            Self::GlobalObject => 4,
            Self::Statement(_) => 5,
        }
    }

    fn encode(self, bytes: &mut ManifestBytes) -> Result<(), ReplayIndexGenerationError> {
        bytes.u8(self.tag());
        match self {
            Self::TypeGroup(id) => bytes.u32(id.0),
            Self::Value(id) => bytes.u32(id.0),
            Self::Namespace(id) => bytes.u32(id.0),
            Self::Class(id) => bytes.u32(id.0),
            Self::GlobalObject => {}
            Self::Statement(key) => {
                bytes.usize(key.file_ordinal.index())?;
                bytes.u32(key.source_start);
                bytes.usize(key.event_ordinal)?;
                bytes.usize(key.record_ordinal)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayOwnerSite {
    pub owner: ReplayOwner,
    pub file_ordinal: LibraryFileOrdinal,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollisionReplayOwnerSite {
    pub owner: ReplayOwner,
    pub file_ordinal: LibraryFileOrdinal,
    pub span: Span,
    pub provenance: CollisionReplaySiteProvenance,
}

pub(crate) fn canonicalize_collision_replay_owner_sites(sites: &mut [CollisionReplayOwnerSite]) {
    sites.sort_by(compare_collision_replay_owner_sites);
}

#[cfg(any(test, feature = "test-utils"))]
pub fn canonicalize_collision_replay_owner_sites_for_test(sites: &mut [CollisionReplayOwnerSite]) {
    canonicalize_collision_replay_owner_sites(sites);
}

#[cfg(any(test, feature = "test-utils"))]
pub(crate) fn collision_replay_owner_sites_are_canonical(
    sites: &[CollisionReplayOwnerSite],
) -> bool {
    sites.windows(2).all(|pair| {
        compare_collision_replay_owner_sites(&pair[0], &pair[1]) != std::cmp::Ordering::Greater
    })
}

#[cfg(any(test, feature = "test-utils"))]
pub fn owner_site_order_controls_for_test() -> (bool, bool) {
    let owner = ReplayOwner::GlobalObject;
    let file_ordinal = LibraryFileOrdinal::new(0);
    let span = Span::new(4, 9);
    let immediate = CollisionReplayOwnerSite {
        owner,
        file_ordinal,
        span,
        provenance: CollisionReplaySiteProvenance::Event {
            phase: CollisionReplayEventPhase::Immediate,
        },
    };
    let deferred = CollisionReplayOwnerSite {
        owner,
        file_ordinal,
        span,
        provenance: CollisionReplaySiteProvenance::Event {
            phase: CollisionReplayEventPhase::Deferred,
        },
    };
    let canonical = vec![immediate.clone(), deferred.clone()];
    let mut reverse_encounter = vec![deferred, immediate];
    let total = compare_collision_replay_owner_sites(&canonical[0], &canonical[1])
        == std::cmp::Ordering::Less
        && collision_replay_owner_sites_are_canonical(&canonical);
    let rejected = !collision_replay_owner_sites_are_canonical(&reverse_encounter);
    canonicalize_collision_replay_owner_sites(&mut reverse_encounter);
    (total, rejected && reverse_encounter == canonical)
}

fn compare_collision_replay_owner_sites(
    left: &CollisionReplayOwnerSite,
    right: &CollisionReplayOwnerSite,
) -> std::cmp::Ordering {
    left.owner
        .cmp(&right.owner)
        .then_with(|| left.file_ordinal.cmp(&right.file_ordinal))
        .then_with(|| left.span.start.cmp(&right.span.start))
        .then_with(|| left.span.end.cmp(&right.span.end))
        .then_with(|| compare_collision_replay_provenance(&left.provenance, &right.provenance))
}

fn compare_collision_replay_provenance(
    left: &CollisionReplaySiteProvenance,
    right: &CollisionReplaySiteProvenance,
) -> std::cmp::Ordering {
    provenance_tag(left)
        .cmp(&provenance_tag(right))
        .then_with(|| match (left, right) {
            (
                CollisionReplaySiteProvenance::Declaration {
                    declaration: left_declaration,
                    kind: left_kind,
                    binder_owner: left_owner,
                    containing_namespace: left_namespace,
                },
                CollisionReplaySiteProvenance::Declaration {
                    declaration: right_declaration,
                    kind: right_kind,
                    binder_owner: right_owner,
                    containing_namespace: right_namespace,
                },
            ) => declaration_kind_tag(*left_kind)
                .cmp(&declaration_kind_tag(*right_kind))
                .then_with(|| compare_declaration_owner(*left_owner, *right_owner))
                .then_with(|| left_namespace.cmp(right_namespace))
                .then_with(|| left_declaration.0.cmp(&right_declaration.0)),
            (
                CollisionReplaySiteProvenance::Event { phase: left },
                CollisionReplaySiteProvenance::Event { phase: right },
            ) => event_phase_tag(*left).cmp(&event_phase_tag(*right)),
            (
                CollisionReplaySiteProvenance::GlobalContributor {
                    name: left_name,
                    kind: left_kind,
                    binder_owner: left_owner,
                },
                CollisionReplaySiteProvenance::GlobalContributor {
                    name: right_name,
                    kind: right_kind,
                    binder_owner: right_owner,
                },
            ) => left_name
                .cmp(right_name)
                .then_with(|| {
                    global_contributor_kind_tag(*left_kind)
                        .cmp(&global_contributor_kind_tag(*right_kind))
                })
                .then_with(|| compare_declaration_owner(*left_owner, *right_owner)),
            (
                CollisionReplaySiteProvenance::ExplicitGlobalThis { binder_owner: left },
                CollisionReplaySiteProvenance::ExplicitGlobalThis {
                    binder_owner: right,
                },
            ) => compare_declaration_owner(*left, *right),
            _ => std::cmp::Ordering::Equal,
        })
}

const fn provenance_tag(provenance: &CollisionReplaySiteProvenance) -> u8 {
    match provenance {
        CollisionReplaySiteProvenance::Declaration { .. } => 0,
        CollisionReplaySiteProvenance::Event { .. } => 1,
        CollisionReplaySiteProvenance::GlobalContributor { .. } => 2,
        CollisionReplaySiteProvenance::ExplicitGlobalThis { .. } => 3,
    }
}

const fn declaration_kind_tag(kind: DeclarationKind) -> u8 {
    match kind {
        DeclarationKind::Variable => 0,
        DeclarationKind::Function => 1,
        DeclarationKind::Class => 2,
        DeclarationKind::Parameter => 3,
        DeclarationKind::CatchParameter => 4,
        DeclarationKind::Import => 5,
        DeclarationKind::TypeAlias => 6,
        DeclarationKind::Interface => 7,
        DeclarationKind::Enum => 8,
        DeclarationKind::Namespace => 9,
        DeclarationKind::ImportEquals => 10,
        DeclarationKind::NamespaceExport => 11,
        DeclarationKind::Global => 12,
    }
}

const fn event_phase_tag(phase: CollisionReplayEventPhase) -> u8 {
    match phase {
        CollisionReplayEventPhase::Immediate => 0,
        CollisionReplayEventPhase::Deferred => 1,
        CollisionReplayEventPhase::Incomplete => 2,
        CollisionReplayEventPhase::Body => 3,
    }
}

const fn global_contributor_kind_tag(kind: SourceGlobalContributorKind) -> u8 {
    match kind {
        SourceGlobalContributorKind::Ordinary => 0,
        SourceGlobalContributorKind::Namespace => 1,
    }
}

fn compare_declaration_owner(
    left: DeclarationOwner,
    right: DeclarationOwner,
) -> std::cmp::Ordering {
    declaration_owner_key(left).cmp(&declaration_owner_key(right))
}

const fn declaration_owner_key(owner: DeclarationOwner) -> (u8, u32) {
    match owner {
        DeclarationOwner::Lexical(scope) => (0, scope.0),
        DeclarationOwner::NamespacePublic(namespace) => (1, namespace.0),
        DeclarationOwner::NamespacePrivate(fragment) => (2, fragment.0),
        DeclarationOwner::CompilationGlobal => (3, 0),
        DeclarationOwner::DeferredAmbientModule(module) => (4, module.0),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollisionReplayEventPhase {
    Immediate,
    Deferred,
    Incomplete,
    Body,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CollisionReplaySiteProvenance {
    Declaration {
        declaration: crate::binder::declaration::DeclId,
        kind: DeclarationKind,
        binder_owner: DeclarationOwner,
        containing_namespace: Option<NamespaceId>,
    },
    Event {
        phase: CollisionReplayEventPhase,
    },
    GlobalContributor {
        name: String,
        kind: SourceGlobalContributorKind,
        binder_owner: DeclarationOwner,
    },
    ExplicitGlobalThis {
        binder_owner: DeclarationOwner,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OwnerSiteStorageMode {
    #[default]
    Flat,
    #[cfg(any(test, feature = "test-utils"))]
    Nested,
    #[cfg(any(test, feature = "test-utils"))]
    Ordered,
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum OrderedOwnerSiteKey {
    Ticket { event: usize, record: usize },
    Direct(usize),
}

#[derive(Debug)]
struct ReplayTicketSlot {
    owner: LibraryEventKey,
    site: Option<CollisionReplayOwnerSite>,
}

#[derive(Clone, Copy, Debug)]
struct ReplayEventTicketRange {
    start: usize,
    len: usize,
}

#[derive(Debug)]
enum ReplayOwnerSiteStorage {
    Flat {
        ticket_slots: Vec<ReplayTicketSlot>,
        event_ranges: Vec<ReplayEventTicketRange>,
        direct_sites: Vec<CollisionReplayOwnerSite>,
        writes: u64,
        valid: bool,
    },
    #[cfg(any(test, feature = "test-utils"))]
    Nested {
        ticket_owners: BTreeMap<(usize, usize), LibraryEventKey>,
        ticket_sites: Vec<Vec<Option<CollisionReplayOwnerSite>>>,
        direct_sites: Vec<CollisionReplayOwnerSite>,
        writes: u64,
        ticket_owner_inserts: u64,
        inner_heap_allocations: u64,
        valid: bool,
    },
    #[cfg(any(test, feature = "test-utils"))]
    Ordered {
        ticket_owners: BTreeMap<(usize, usize), LibraryEventKey>,
        sites: BTreeMap<OrderedOwnerSiteKey, CollisionReplayOwnerSite>,
        next_direct: usize,
        inserts: u64,
        ticket_owner_inserts: u64,
        valid: bool,
    },
}

struct FinishedReplayOwnerSiteStorage {
    ticket_owners: Vec<LibraryEventKey>,
    owner_sites: Vec<CollisionReplayOwnerSite>,
    ticket_slots: u64,
    ticket_owner_ordered_map_inserts: u64,
    owner_site_inner_heap_allocations: u64,
    dense_slot_writes: u64,
    ordered_map_inserts: u64,
}

impl ReplayOwnerSiteStorage {
    fn new(mode: OwnerSiteStorageMode) -> Self {
        match mode {
            OwnerSiteStorageMode::Flat => Self::Flat {
                ticket_slots: Vec::new(),
                event_ranges: Vec::new(),
                direct_sites: Vec::new(),
                writes: 0,
                valid: true,
            },
            #[cfg(any(test, feature = "test-utils"))]
            OwnerSiteStorageMode::Nested => Self::Nested {
                ticket_owners: BTreeMap::new(),
                ticket_sites: Vec::new(),
                direct_sites: Vec::new(),
                writes: 0,
                ticket_owner_inserts: 0,
                inner_heap_allocations: 0,
                valid: true,
            },
            #[cfg(any(test, feature = "test-utils"))]
            OwnerSiteStorageMode::Ordered => Self::Ordered {
                ticket_owners: BTreeMap::new(),
                sites: BTreeMap::new(),
                next_direct: 0,
                inserts: 0,
                ticket_owner_inserts: 0,
                valid: true,
            },
        }
    }

    fn reserve_ticket(&mut self, ticket: (usize, usize), owner: LibraryEventKey) -> bool {
        match self {
            Self::Flat {
                ticket_slots,
                event_ranges,
                valid,
                ..
            } => {
                let contiguous = if ticket.1 == 0 {
                    ticket.0 == event_ranges.len()
                } else {
                    event_ranges.get(ticket.0).is_some_and(|range| {
                        ticket.0.checked_add(1) == Some(event_ranges.len())
                            && ticket.1 == range.len
                            && range.start.checked_add(range.len) == Some(ticket_slots.len())
                            && ticket_slots.get(range.start).is_some_and(|primary| {
                                primary.owner.file_ordinal == owner.file_ordinal
                                    && primary.owner.source_start == owner.source_start
                                    && primary.owner.event_ordinal == owner.event_ordinal
                            })
                    })
                } && owner.record_ordinal == ticket.1;
                if !contiguous {
                    *valid = false;
                    return false;
                }
                if ticket.1 == 0 {
                    event_ranges.push(ReplayEventTicketRange {
                        start: ticket_slots.len(),
                        len: 1,
                    });
                } else if let Some(range) = event_ranges.get_mut(ticket.0) {
                    let Some(next_len) = range.len.checked_add(1) else {
                        *valid = false;
                        return false;
                    };
                    range.len = next_len;
                }
                ticket_slots.push(ReplayTicketSlot { owner, site: None });
                true
            }
            #[cfg(any(test, feature = "test-utils"))]
            Self::Nested {
                ticket_owners,
                ticket_sites,
                ticket_owner_inserts,
                inner_heap_allocations,
                valid,
                ..
            } => {
                if ticket_owners.contains_key(&ticket) {
                    *valid = false;
                    return false;
                }
                ticket_owners.insert(ticket, owner);
                *ticket_owner_inserts = ticket_owner_inserts.saturating_add(1);
                while ticket_sites.len() <= ticket.0 {
                    ticket_sites.push(Vec::new());
                }
                if let Some(event_sites) = ticket_sites.get_mut(ticket.0) {
                    let required = ticket.1.saturating_add(1);
                    if event_sites.len() < required {
                        if event_sites.capacity() == 0 {
                            *inner_heap_allocations = inner_heap_allocations.saturating_add(1);
                        }
                        event_sites.resize_with(required, || None);
                    }
                }
                true
            }
            #[cfg(any(test, feature = "test-utils"))]
            Self::Ordered {
                ticket_owners,
                ticket_owner_inserts,
                valid,
                ..
            } => {
                if ticket_owners.contains_key(&ticket) {
                    *valid = false;
                    return false;
                }
                ticket_owners.insert(ticket, owner);
                *ticket_owner_inserts = ticket_owner_inserts.saturating_add(1);
                true
            }
        }
    }

    fn ticket_reservations_are_valid(&self) -> bool {
        match self {
            Self::Flat { valid, .. } => *valid,
            #[cfg(any(test, feature = "test-utils"))]
            Self::Nested { valid, .. } | Self::Ordered { valid, .. } => *valid,
        }
    }

    fn ticket_count(&self) -> usize {
        match self {
            Self::Flat { ticket_slots, .. } => ticket_slots.len(),
            #[cfg(any(test, feature = "test-utils"))]
            Self::Nested { ticket_owners, .. } | Self::Ordered { ticket_owners, .. } => {
                ticket_owners.len()
            }
        }
    }

    fn ticket_owner(&self, ticket: (usize, usize)) -> Option<LibraryEventKey> {
        match self {
            Self::Flat {
                ticket_slots,
                event_ranges,
                ..
            } => {
                let range = event_ranges.get(ticket.0)?;
                if ticket.1 >= range.len {
                    return None;
                }
                let index = range.start.checked_add(ticket.1)?;
                ticket_slots.get(index).map(|slot| slot.owner)
            }
            #[cfg(any(test, feature = "test-utils"))]
            Self::Nested { ticket_owners, .. } | Self::Ordered { ticket_owners, .. } => {
                ticket_owners.get(&ticket).copied()
            }
        }
    }

    fn ticket_owners(&self) -> Vec<LibraryEventKey> {
        match self {
            Self::Flat { ticket_slots, .. } => ticket_slots.iter().map(|slot| slot.owner).collect(),
            #[cfg(any(test, feature = "test-utils"))]
            Self::Nested { ticket_owners, .. } | Self::Ordered { ticket_owners, .. } => {
                ticket_owners.values().copied().collect()
            }
        }
    }

    fn contains_ticket(&self, ticket: (usize, usize)) -> bool {
        match self {
            Self::Flat {
                ticket_slots,
                event_ranges,
                ..
            } => event_ranges
                .get(ticket.0)
                .filter(|range| ticket.1 < range.len)
                .and_then(|range| range.start.checked_add(ticket.1))
                .and_then(|index| ticket_slots.get(index))
                .is_some_and(|slot| slot.site.is_some()),
            #[cfg(any(test, feature = "test-utils"))]
            Self::Nested { ticket_sites, .. } => ticket_sites
                .get(ticket.0)
                .and_then(|event| event.get(ticket.1))
                .is_some_and(Option::is_some),
            #[cfg(any(test, feature = "test-utils"))]
            Self::Ordered { sites, .. } => sites.contains_key(&OrderedOwnerSiteKey::Ticket {
                event: ticket.0,
                record: ticket.1,
            }),
        }
    }

    fn missing_ticket_site_count(&self) -> usize {
        match self {
            Self::Flat { ticket_slots, .. } => ticket_slots
                .iter()
                .filter(|slot| slot.site.is_none())
                .count(),
            #[cfg(any(test, feature = "test-utils"))]
            Self::Nested {
                ticket_owners,
                ticket_sites,
                ..
            } => ticket_owners
                .keys()
                .filter(|ticket| {
                    ticket_sites
                        .get(ticket.0)
                        .and_then(|event| event.get(ticket.1))
                        .is_none_or(Option::is_none)
                })
                .count(),
            #[cfg(any(test, feature = "test-utils"))]
            Self::Ordered {
                ticket_owners,
                sites,
                ..
            } => ticket_owners
                .keys()
                .filter(|ticket| {
                    !sites.contains_key(&OrderedOwnerSiteKey::Ticket {
                        event: ticket.0,
                        record: ticket.1,
                    })
                })
                .count(),
        }
    }

    fn record_ticket(
        &mut self,
        ticket: (usize, usize),
        site: CollisionReplayOwnerSite,
    ) -> Option<bool> {
        match self {
            Self::Flat {
                ticket_slots,
                event_ranges,
                writes,
                ..
            } => {
                let range = event_ranges.get(ticket.0)?;
                if ticket.1 >= range.len {
                    return None;
                }
                let index = range.start.checked_add(ticket.1)?;
                let slot = ticket_slots.get_mut(index)?;
                if slot.site.is_some() {
                    return Some(true);
                }
                slot.site = Some(site);
                *writes = writes.saturating_add(1);
                Some(false)
            }
            #[cfg(any(test, feature = "test-utils"))]
            Self::Nested {
                ticket_sites,
                writes,
                ..
            } => {
                let slot = ticket_sites
                    .get_mut(ticket.0)
                    .and_then(|event| event.get_mut(ticket.1))?;
                if slot.is_some() {
                    return Some(true);
                }
                *slot = Some(site);
                *writes = writes.saturating_add(1);
                Some(false)
            }
            #[cfg(any(test, feature = "test-utils"))]
            Self::Ordered { sites, inserts, .. } => {
                let key = OrderedOwnerSiteKey::Ticket {
                    event: ticket.0,
                    record: ticket.1,
                };
                if sites.contains_key(&key) {
                    return Some(true);
                }
                sites.insert(key, site);
                *inserts = inserts.saturating_add(1);
                Some(false)
            }
        }
    }

    fn record_direct(&mut self, site: CollisionReplayOwnerSite) {
        match self {
            Self::Flat {
                direct_sites,
                writes,
                ..
            } => {
                direct_sites.push(site);
                *writes = writes.saturating_add(1);
            }
            #[cfg(any(test, feature = "test-utils"))]
            Self::Nested {
                direct_sites,
                writes,
                ..
            } => {
                direct_sites.push(site);
                *writes = writes.saturating_add(1);
            }
            #[cfg(any(test, feature = "test-utils"))]
            Self::Ordered {
                sites,
                next_direct,
                inserts,
                ..
            } => {
                sites.insert(OrderedOwnerSiteKey::Direct(*next_direct), site);
                *next_direct = next_direct.saturating_add(1);
                *inserts = inserts.saturating_add(1);
            }
        }
    }

    fn extend_direct(&mut self, sites: impl IntoIterator<Item = CollisionReplayOwnerSite>) {
        for site in sites {
            self.record_direct(site);
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn ticket_site_and_write_counts(
        &self,
        ticket: (usize, usize),
    ) -> (Option<&CollisionReplayOwnerSite>, u64, u64) {
        match self {
            Self::Flat {
                ticket_slots,
                event_ranges,
                writes,
                ..
            } => {
                let captured = event_ranges
                    .get(ticket.0)
                    .filter(|range| ticket.1 < range.len)
                    .and_then(|range| range.start.checked_add(ticket.1))
                    .and_then(|index| ticket_slots.get(index))
                    .and_then(|slot| slot.site.as_ref());
                (captured, *writes, 0)
            }
            Self::Nested {
                ticket_sites,
                writes,
                ..
            } => (
                ticket_sites
                    .get(ticket.0)
                    .and_then(|event| event.get(ticket.1))
                    .and_then(Option::as_ref),
                *writes,
                0,
            ),
            Self::Ordered { sites, inserts, .. } => (
                sites.get(&OrderedOwnerSiteKey::Ticket {
                    event: ticket.0,
                    record: ticket.1,
                }),
                0,
                *inserts,
            ),
        }
    }

    fn finish(self) -> FinishedReplayOwnerSiteStorage {
        match self {
            Self::Flat {
                ticket_slots,
                direct_sites,
                writes,
                ..
            } => {
                let ticket_count = ticket_slots.len();
                let mut ticket_owners = Vec::with_capacity(ticket_count);
                let mut owner_sites =
                    Vec::with_capacity(ticket_count.saturating_add(direct_sites.len()));
                for slot in ticket_slots {
                    ticket_owners.push(slot.owner);
                    if let Some(site) = slot.site {
                        owner_sites.push(site);
                    }
                }
                owner_sites.extend(direct_sites);
                FinishedReplayOwnerSiteStorage {
                    ticket_owners,
                    owner_sites,
                    ticket_slots: u64::try_from(ticket_count).unwrap_or(u64::MAX),
                    ticket_owner_ordered_map_inserts: 0,
                    owner_site_inner_heap_allocations: 0,
                    dense_slot_writes: writes,
                    ordered_map_inserts: 0,
                }
            }
            #[cfg(any(test, feature = "test-utils"))]
            Self::Nested {
                ticket_owners,
                ticket_sites,
                direct_sites,
                writes,
                ticket_owner_inserts,
                inner_heap_allocations,
                ..
            } => {
                let mut owner_sites = ticket_sites
                    .into_iter()
                    .flatten()
                    .flatten()
                    .collect::<Vec<_>>();
                owner_sites.extend(direct_sites);
                FinishedReplayOwnerSiteStorage {
                    ticket_slots: u64::try_from(ticket_owners.len()).unwrap_or(u64::MAX),
                    ticket_owners: ticket_owners.into_values().collect(),
                    owner_sites,
                    ticket_owner_ordered_map_inserts: ticket_owner_inserts,
                    owner_site_inner_heap_allocations: inner_heap_allocations,
                    dense_slot_writes: writes,
                    ordered_map_inserts: 0,
                }
            }
            #[cfg(any(test, feature = "test-utils"))]
            Self::Ordered {
                ticket_owners,
                sites,
                inserts,
                ticket_owner_inserts,
                ..
            } => FinishedReplayOwnerSiteStorage {
                ticket_slots: u64::try_from(ticket_owners.len()).unwrap_or(u64::MAX),
                ticket_owners: ticket_owners.into_values().collect(),
                owner_sites: sites.into_values().collect(),
                ticket_owner_ordered_map_inserts: ticket_owner_inserts,
                owner_site_inner_heap_allocations: 0,
                dense_slot_writes: 0,
                ordered_map_inserts: inserts,
            },
        }
    }
}

#[derive(Debug)]
pub struct ReplayTraceSeed {
    owner_sites: ReplayOwnerSiteStorage,
    pub duplicate_owner_site_count: u64,
    pub missing_owner_site_count: u64,
    pub trace_domain_sealed_after_binder_reporting: bool,
}

impl ReplayTraceSeed {
    pub fn new(mode: OwnerSiteStorageMode) -> Self {
        Self {
            owner_sites: ReplayOwnerSiteStorage::new(mode),
            duplicate_owner_site_count: 0,
            missing_owner_site_count: 0,
            trace_domain_sealed_after_binder_reporting: false,
        }
    }

    pub fn reserve_owner_site_ticket(
        &mut self,
        ticket: (usize, usize),
        owner: LibraryEventKey,
    ) -> bool {
        self.owner_sites.reserve_ticket(ticket, owner)
    }

    pub fn ticket_reservations_are_valid(&self) -> bool {
        self.owner_sites.ticket_reservations_are_valid()
    }

    pub fn ticket_count(&self) -> usize {
        self.owner_sites.ticket_count()
    }

    pub fn ticket_owner(&self, ticket: (usize, usize)) -> Option<LibraryEventKey> {
        self.owner_sites.ticket_owner(ticket)
    }

    pub fn contains_owner_site_ticket(&self, ticket: (usize, usize)) -> bool {
        self.owner_sites.contains_ticket(ticket)
    }

    pub fn missing_owner_site_ticket_count(&self) -> usize {
        self.owner_sites.missing_ticket_site_count()
    }

    pub fn record_ticket_owner_site(
        &mut self,
        ticket: (usize, usize),
        site: CollisionReplayOwnerSite,
    ) -> Option<bool> {
        self.owner_sites.record_ticket(ticket, site)
    }

    pub fn extend_owner_sites(
        &mut self,
        sites: impl IntoIterator<Item = CollisionReplayOwnerSite>,
    ) {
        self.owner_sites.extend_direct(sites);
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn duplicate_write_control_for_test(
        self,
        ticket: (usize, usize),
        expected_first: &CollisionReplayOwnerSite,
    ) -> (bool, bool) {
        let (captured, dense_writes, ordered_inserts) =
            self.owner_sites.ticket_site_and_write_counts(ticket);
        let first_preserved =
            captured == Some(expected_first) && dense_writes.saturating_add(ordered_inserts) == 1;
        let rejected = matches!(
            ReplayDependencyTrace::new(self).finish_compact_plan(
                Vec::new(),
                Vec::new(),
                [1; 9],
                CollisionReplayConstructionEvidence::default(),
            ),
            Err(ReplayIndexGenerationError::InvalidOwnerSites(1))
        );
        (rejected, first_preserved)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayRootSlot {
    pub name: String,
    pub value: Option<ValueStorageId>,
    pub ty: Option<TypeGroupId>,
    pub namespace: Option<NamespaceId>,
    pub global_object_contributor: bool,
    pub explicit_global_this: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReplayReverseEdge {
    pub dependency: ReplayOwner,
    pub consumer: ReplayOwner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RootSlotKind {
    Value,
    Type,
    Namespace,
}

impl RootSlotKind {
    const fn tag(self) -> u8 {
        match self {
            Self::Value => 0,
            Self::Type => 1,
            Self::Namespace => 2,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReplayDependencyKey {
    Owner(ReplayOwner),
    RootSlot { name: String, slot: RootSlotKind },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReplayRootConsumer {
    pub name: String,
    pub slot: RootSlotKind,
    pub consumer: ReplayOwner,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayScc {
    pub replay_ordinal: u32,
    pub owners: SmallVec<[ReplayOwner; 1]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayBaselineRecord {
    pub owner: ReplayOwner,
    pub record_count: u64,
    pub digest: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplayBaselineValidationError {
    MissingExpected(ReplayBaselineRecord),
    Unexpected(ReplayBaselineRecord),
    Changed {
        expected: Box<ReplayBaselineRecord>,
        observed: Box<ReplayBaselineRecord>,
    },
}

pub fn validate_replay_baselines(
    expected: &[ReplayBaselineRecord],
    observed: &[ReplayBaselineRecord],
) -> Result<(), ReplayBaselineValidationError> {
    let mut expected_index = 0;
    let mut observed_index = 0;
    while let (Some(expected_record), Some(observed_record)) =
        (expected.get(expected_index), observed.get(observed_index))
    {
        match expected_record.owner.cmp(&observed_record.owner) {
            std::cmp::Ordering::Less => {
                return Err(ReplayBaselineValidationError::MissingExpected(
                    expected_record.clone(),
                ));
            }
            std::cmp::Ordering::Greater => {
                return Err(ReplayBaselineValidationError::Unexpected(
                    observed_record.clone(),
                ));
            }
            std::cmp::Ordering::Equal if expected_record != observed_record => {
                return Err(ReplayBaselineValidationError::Changed {
                    expected: Box::new(expected_record.clone()),
                    observed: Box::new(observed_record.clone()),
                });
            }
            std::cmp::Ordering::Equal => {
                expected_index += 1;
                observed_index += 1;
            }
        }
    }
    if let Some(record) = expected.get(expected_index) {
        return Err(ReplayBaselineValidationError::MissingExpected(
            record.clone(),
        ));
    }
    if let Some(record) = observed.get(observed_index) {
        return Err(ReplayBaselineValidationError::Unexpected(record.clone()));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollisionReplayIndex {
    pub schema: u32,
    pub owner_partition: Vec<ReplayOwner>,
    pub root_slots: Vec<ReplayRootSlot>,
    pub owner_sites: Vec<ReplayOwnerSite>,
    pub reverse_edges: Vec<ReplayReverseEdge>,
    pub root_slot_consumers: Vec<ReplayRootConsumer>,
    pub scc_membership: Vec<ReplayScc>,
    pub statement_owners: Vec<(LibraryEventKey, ReplayOwner)>,
    pub baseline_records: Vec<ReplayBaselineRecord>,
    pub unowned_demand_count: u64,
    pub invalid_owner_site_count: u64,
    pub noncanonical_edge_count: u64,
    pub typed_reference_coverage_misses: u64,
    pub canonical_manifest_bytes: Vec<u8>,
    pub canonical_manifest_sha256: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollisionReplayPrefixBoundary {
    pub cardinality: usize,
    pub exact: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CollisionReplayPlanHealth {
    pub missing_active_owner_demands: u64,
    pub unmatched_owner_sites: u64,
    pub unowned_typed_references: u64,
    pub raw_semantic_accesses: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CollisionReplayConstructionEvidence {
    pub library_source_compiles: u64,
    pub binder_source_censuses: u64,
    pub canonical_source_units: u64,
    pub second_source_censuses: u64,
    pub canonical_manifest_bytes: u64,
    pub rendered_record_digest_bytes: u64,
    pub transitive_terminal_owner_entries: u64,
    pub eager_all_owner_scc_memberships: u64,
    pub namespace_snapshot_rows: u64,
    pub runtime_snapshot_rows: u64,
    pub canonical_terminal_rows: u64,
    pub full_semantic_projection_rows: u64,
    pub ticket_slots: u64,
    pub ticket_owner_ordered_map_inserts: u64,
    pub owner_site_inner_heap_allocations: u64,
    pub owner_site_dense_slot_writes: u64,
    pub owner_site_ordered_map_inserts: u64,
    pub trace_domain_sealed_after_binder_reporting: bool,
}

pub struct CollisionReplayPlan {
    root_names: Vec<String>,
    root_slots: Vec<ReplayRootSlot>,
    owner_sites: Vec<CollisionReplayOwnerSite>,
    reverse_edges: Vec<ReplayReverseEdge>,
    root_slot_consumers: Vec<ReplayRootConsumer>,
    root_slot_owners: Vec<ReplayOwner>,
    namespace_direct_owners: Vec<(NamespaceId, ReplayOwner)>,
    statement_owners: Vec<(LibraryEventKey, ReplayOwner)>,
    baseline_records: Vec<ReplayBaselineRecord>,
    prefix_boundaries: [CollisionReplayPrefixBoundary; 9],
    health: CollisionReplayPlanHealth,
    construction: CollisionReplayConstructionEvidence,
    sealed_digest: Option<[u8; 32]>,
}

impl std::fmt::Debug for CollisionReplayPlan {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CollisionReplayPlan")
            .field("root_names", &self.root_names.len())
            .field("root_slots", &self.root_slots.len())
            .field("owner_sites", &self.owner_sites.len())
            .field("reverse_edges", &self.reverse_edges.len())
            .field("root_slot_consumers", &self.root_slot_consumers.len())
            .field("root_slot_owners", &self.root_slot_owners.len())
            .field(
                "namespace_direct_owners",
                &self.namespace_direct_owners.len(),
            )
            .field("statement_owners", &self.statement_owners.len())
            .field("baseline_records", &self.baseline_records.len())
            .field("prefix_boundaries", &self.prefix_boundaries)
            .field("health", &self.health)
            .field("construction", &self.construction)
            .field("sealed", &self.sealed_digest.is_some())
            .finish()
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollisionReplayPlanFullSnapshotForTest {
    pub root_names: Vec<String>,
    pub root_slots: Vec<ReplayRootSlot>,
    pub owner_sites: Vec<CollisionReplayOwnerSite>,
    pub reverse_edges: Vec<ReplayReverseEdge>,
    pub root_slot_consumers: Vec<ReplayRootConsumer>,
    pub root_slot_owners: Vec<ReplayOwner>,
    pub namespace_direct_owners: Vec<(NamespaceId, ReplayOwner)>,
    pub statement_owners: Vec<(LibraryEventKey, ReplayOwner)>,
    pub baseline_records: Vec<ReplayBaselineRecord>,
    pub prefix_boundaries: [CollisionReplayPrefixBoundary; 9],
}

pub struct CompleteCollisionReplayPlanMaterialization {
    pub owners: BTreeSet<ReplayOwner>,
    pub owner_sites: Vec<CollisionReplayOwnerSite>,
    pub baseline_records: Vec<ReplayBaselineRecord>,
}

impl CollisionReplayPlan {
    pub fn root_slot(&self, name: &str) -> Option<&ReplayRootSlot> {
        self.root_slots
            .binary_search_by(|root| root.name.as_str().cmp(name))
            .ok()
            .and_then(|index| self.root_slots.get(index))
    }

    pub fn root_consumers(&self, name: &str) -> &[ReplayRootConsumer] {
        let first = self
            .root_slot_consumers
            .partition_point(|consumer| consumer.name.as_str() < name);
        let last = self
            .root_slot_consumers
            .partition_point(|consumer| consumer.name.as_str() <= name);
        &self.root_slot_consumers[first..last]
    }

    pub fn namespace_direct_owners(
        &self,
        namespace: NamespaceId,
    ) -> impl Iterator<Item = ReplayOwner> + '_ {
        let first = self
            .namespace_direct_owners
            .partition_point(|(owner, _)| *owner < namespace);
        let last = self
            .namespace_direct_owners
            .partition_point(|(owner, _)| *owner <= namespace);
        self.namespace_direct_owners[first..last]
            .iter()
            .map(|(_, owner)| *owner)
    }

    pub fn reverse_consumers(&self, owner: ReplayOwner) -> &[ReplayReverseEdge] {
        let first = self
            .reverse_edges
            .partition_point(|edge| edge.dependency < owner);
        let last = self
            .reverse_edges
            .partition_point(|edge| edge.dependency <= owner);
        &self.reverse_edges[first..last]
    }

    pub fn owner_sites_for(&self, owner: ReplayOwner) -> &[CollisionReplayOwnerSite] {
        let first = self.owner_sites.partition_point(|site| site.owner < owner);
        let last = self.owner_sites.partition_point(|site| site.owner <= owner);
        &self.owner_sites[first..last]
    }

    pub fn baselines_for(&self, owner: ReplayOwner) -> &[ReplayBaselineRecord] {
        let first = self
            .baseline_records
            .partition_point(|record| record.owner < owner);
        let last = self
            .baseline_records
            .partition_point(|record| record.owner <= owner);
        &self.baseline_records[first..last]
    }

    pub fn contains_mutation_owner(&self, owner: ReplayOwner) -> bool {
        self.root_slot_owners.binary_search(&owner).is_ok()
            || self
                .owner_sites
                .binary_search_by(|site| site.owner.cmp(&owner))
                .is_ok()
    }

    pub fn prefix_cardinality(&self, family: usize) -> Option<usize> {
        self.prefix_boundaries
            .get(family)
            .map(|boundary| boundary.cardinality)
    }

    pub fn health(&self) -> CollisionReplayPlanHealth {
        self.health
    }

    pub fn construction(&self) -> CollisionReplayConstructionEvidence {
        self.construction
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn full_row_count(&self) -> usize {
        self.root_names
            .len()
            .saturating_add(self.root_slots.len())
            .saturating_add(self.owner_sites.len())
            .saturating_add(self.reverse_edges.len())
            .saturating_add(self.root_slot_consumers.len())
            .saturating_add(self.root_slot_owners.len())
            .saturating_add(self.namespace_direct_owners.len())
            .saturating_add(self.statement_owners.len())
            .saturating_add(self.baseline_records.len())
            .saturating_add(self.prefix_boundaries.len())
    }

    pub fn complete_source_materialization(&self) -> CompleteCollisionReplayPlanMaterialization {
        #[cfg(any(test, feature = "test-utils"))]
        super::library_compiler::record_private_replay_full_base_scan_for_test(
            self.owner_sites
                .len()
                .saturating_add(self.baseline_records.len().saturating_mul(2)),
        );
        CompleteCollisionReplayPlanMaterialization {
            owners: self
                .baseline_records
                .iter()
                .map(|record| record.owner)
                .collect(),
            owner_sites: self.owner_sites.clone(),
            baseline_records: self.baseline_records.clone(),
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn measured_full_scan_for_test(&self) -> usize {
        let rows = self.full_row_count();
        super::library_compiler::record_private_replay_full_base_scan_for_test(rows);
        rows
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn full_oracle_snapshot_for_test(&self) -> CollisionReplayPlanFullSnapshotForTest {
        self.measured_full_scan_for_test();
        CollisionReplayPlanFullSnapshotForTest {
            root_names: self.root_names.clone(),
            root_slots: self.root_slots.clone(),
            owner_sites: self.owner_sites.clone(),
            reverse_edges: self.reverse_edges.clone(),
            root_slot_consumers: self.root_slot_consumers.clone(),
            root_slot_owners: self.root_slot_owners.clone(),
            namespace_direct_owners: self.namespace_direct_owners.clone(),
            statement_owners: self.statement_owners.clone(),
            baseline_records: self.baseline_records.clone(),
            prefix_boundaries: self.prefix_boundaries,
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn clone_with_fault_for_test(
        &self,
        mutation: &str,
        replacement_digest: Option<[u8; 32]>,
    ) -> Result<Self, &'static str> {
        self.measured_full_scan_for_test();
        let mut candidate = Self {
            root_names: self.root_names.clone(),
            root_slots: self.root_slots.clone(),
            owner_sites: self.owner_sites.clone(),
            reverse_edges: self.reverse_edges.clone(),
            root_slot_consumers: self.root_slot_consumers.clone(),
            root_slot_owners: self.root_slot_owners.clone(),
            namespace_direct_owners: self.namespace_direct_owners.clone(),
            statement_owners: self.statement_owners.clone(),
            baseline_records: self.baseline_records.clone(),
            prefix_boundaries: self.prefix_boundaries,
            health: self.health,
            construction: self.construction,
            sealed_digest: self.sealed_digest,
        };
        match mutation {
            "drop-direct-edge" => {
                candidate.reverse_edges.pop();
            }
            "drop-owner-site" => {
                candidate.owner_sites.pop();
            }
            "drop-root-slot-consumer" => {
                candidate.root_slot_consumers.pop();
            }
            "drop-statement-owner" => {
                candidate.statement_owners.pop();
            }
            "drop-record-fingerprint" => {
                candidate.baseline_records.pop();
            }
            "change-record-cardinality" => {
                let record = candidate
                    .baseline_records
                    .first_mut()
                    .ok_or("plan has no record fingerprint")?;
                record.record_count = record.record_count.saturating_add(1);
            }
            "change-record-elaboration" => {
                let digest = replacement_digest.ok_or("replacement digest is absent")?;
                let record = candidate
                    .baseline_records
                    .first_mut()
                    .ok_or("plan has no record fingerprint")?;
                record.digest = digest;
            }
            "change-prefix-boundary" => {
                candidate.prefix_boundaries[0].cardinality =
                    candidate.prefix_boundaries[0].cardinality.saturating_add(1);
            }
            "drop-binder-provenance" => {
                let provenance = candidate
                    .owner_sites
                    .iter_mut()
                    .find_map(|site| match &mut site.provenance {
                        CollisionReplaySiteProvenance::Declaration { binder_owner, .. } => {
                            Some(binder_owner)
                        }
                        _ => None,
                    })
                    .ok_or("plan has no declaration provenance")?;
                *provenance = match *provenance {
                    crate::binder::namespace::DeclarationOwner::CompilationGlobal => {
                        crate::binder::namespace::DeclarationOwner::Lexical(
                            crate::binder::scope::ScopeId(u32::MAX),
                        )
                    }
                    _ => crate::binder::namespace::DeclarationOwner::CompilationGlobal,
                };
            }
            "change-owner-site-kind" => {
                let kind = candidate
                    .owner_sites
                    .iter_mut()
                    .find_map(|site| match &mut site.provenance {
                        CollisionReplaySiteProvenance::Declaration { kind, .. } => Some(kind),
                        _ => None,
                    })
                    .ok_or("plan has no declaration site")?;
                *kind = match *kind {
                    crate::binder::declaration::DeclarationKind::Variable => {
                        crate::binder::declaration::DeclarationKind::Function
                    }
                    _ => crate::binder::declaration::DeclarationKind::Variable,
                };
            }
            "change-owner-site-span-end" => {
                let site = candidate
                    .owner_sites
                    .first_mut()
                    .ok_or("plan has no owner site")?;
                site.span.end = site.span.end.saturating_add(1);
            }
            "duplicate-owner-and-drop-dense-id" => {
                let source_owner = candidate
                    .owner_sites
                    .iter()
                    .map(|site| site.owner)
                    .find(|owner| {
                        matches!(
                            owner,
                            ReplayOwner::TypeGroup(crate::binder::declaration::TypeGroupId(1))
                        )
                    })
                    .ok_or("plan has no second dense type-group owner")?;
                for site in &mut candidate.owner_sites {
                    if site.owner == source_owner {
                        site.owner =
                            ReplayOwner::TypeGroup(crate::binder::declaration::TypeGroupId(0));
                    }
                }
                canonicalize_collision_replay_owner_sites_for_test(&mut candidate.owner_sites);
            }
            "out-of-range-root-owner" => {
                let root = candidate
                    .root_slots
                    .iter_mut()
                    .find(|root| root.ty.is_some())
                    .ok_or("plan has no typed root")?;
                root.ty = Some(crate::binder::declaration::TypeGroupId(u32::MAX));
            }
            "drop-typed-reference" => candidate.health.unowned_typed_references = 1,
            "add-raw-semantic-access" => candidate.health.raw_semantic_accesses = 1,
            "perform-forbidden-projection" => {}
            _ => return Err("unknown collision plan mutation"),
        }
        Ok(candidate)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn append_unreachable_padding_for_test(&mut self, count: usize, profile_identity: &str) {
        self.sealed_digest = None;
        self.reverse_edges.extend((0..count).map(|index| {
            let owner = ReplayOwner::Statement(LibraryEventKey {
                file_ordinal: LibraryFileOrdinal::new(
                    self.construction
                        .canonical_source_units
                        .saturating_add(u64::try_from(index).unwrap_or(u64::MAX))
                        .try_into()
                        .unwrap_or(usize::MAX),
                ),
                source_start: u32::MAX,
                event_ordinal: index,
                record_ordinal: 0,
            });
            ReplayReverseEdge {
                dependency: owner,
                consumer: owner,
            }
        }));
        super::library_compiler::record_private_replay_full_base_scan_for_test(
            self.reverse_edges.len().saturating_mul(2),
        );
        self.reverse_edges.sort_unstable();
        self.reverse_edges.dedup();
        self.sealed_digest = Some(self.admission_digest(profile_identity));
    }

    pub fn seal_for_frozen_base(
        &mut self,
        observed: [usize; 9],
        profile_identity: &str,
    ) -> Result<(), ReplayIndexGenerationError> {
        if self.sealed_digest.is_some() {
            return Err(ReplayIndexGenerationError::BaselinePartitionMismatch);
        }
        let mut mismatch = false;
        for (boundary, observed) in self.prefix_boundaries.iter_mut().zip(observed) {
            boundary.exact = boundary.cardinality == observed;
            mismatch |= !boundary.exact;
        }
        if mismatch || !self.intrinsic_admission_evidence_is_valid() {
            return Err(ReplayIndexGenerationError::BaselinePartitionMismatch);
        }
        self.sealed_digest = Some(self.admission_digest(profile_identity));
        Ok(())
    }

    pub fn admit_for_frozen_base(
        &self,
        observed: [usize; 9],
        profile_identity: &str,
    ) -> Result<(), ReplayIndexGenerationError> {
        let expected = self
            .sealed_digest
            .ok_or(ReplayIndexGenerationError::BaselinePartitionMismatch)?;
        let prefixes_match = self
            .prefix_boundaries
            .iter()
            .zip(observed)
            .all(|(boundary, observed)| boundary.exact && boundary.cardinality == observed);
        if !prefixes_match
            || !self.intrinsic_admission_evidence_is_valid()
            || self.admission_digest(profile_identity) != expected
        {
            return Err(ReplayIndexGenerationError::BaselinePartitionMismatch);
        }
        Ok(())
    }

    pub fn seal_admit_and_materialize_root_name_index(
        &mut self,
        observed: [usize; 9],
        profile_identity: &str,
    ) -> Result<BTreeSet<String>, ReplayIndexGenerationError> {
        self.seal_for_frozen_base(observed, profile_identity)?;
        self.admit_for_frozen_base(observed, profile_identity)?;
        #[cfg(any(test, feature = "test-utils"))]
        super::library_compiler::record_private_replay_full_base_scan_for_test(
            self.root_names.len(),
        );
        Ok(self.root_names.iter().cloned().collect())
    }

    fn intrinsic_admission_evidence_is_valid(&self) -> bool {
        self.health == CollisionReplayPlanHealth::default()
            && self.construction.library_source_compiles == 1
            && self.construction.canonical_source_units != 0
            && self.construction.binder_source_censuses == self.construction.canonical_source_units
            && self.construction.second_source_censuses == 0
            && self.construction.canonical_manifest_bytes == 0
            && self.construction.rendered_record_digest_bytes == 0
            && self.construction.transitive_terminal_owner_entries == 0
            && self.construction.eager_all_owner_scc_memberships == 0
            && self.construction.namespace_snapshot_rows == 0
            && self.construction.runtime_snapshot_rows == 0
            && self.construction.canonical_terminal_rows == 0
            && self.construction.full_semantic_projection_rows == 0
            && self.construction.ticket_slots
                == u64::try_from(self.statement_owners.len()).unwrap_or(u64::MAX)
            && self.construction.ticket_owner_ordered_map_inserts == 0
            && self.construction.owner_site_inner_heap_allocations == 0
            && self.construction.owner_site_dense_slot_writes
                == u64::try_from(self.owner_sites.len()).unwrap_or(u64::MAX)
            && self.construction.owner_site_ordered_map_inserts == 0
            && self.construction.trace_domain_sealed_after_binder_reporting
    }

    fn admission_digest(&self, profile_identity: &str) -> [u8; 32] {
        #[cfg(any(test, feature = "test-utils"))]
        super::library_compiler::record_private_replay_full_base_scan_for_test(
            self.full_row_count(),
        );
        let mut digest = Sha256::new();
        digest.update(b"typokat-collision-replay-plan-admission-v1");
        digest_string(&mut digest, profile_identity);
        digest_usize(&mut digest, self.root_names.len());
        for name in &self.root_names {
            digest_string(&mut digest, name);
        }
        digest_usize(&mut digest, self.root_slots.len());
        for root in &self.root_slots {
            digest_string(&mut digest, &root.name);
            digest_optional_u32(&mut digest, root.value.map(|id| id.0));
            digest_optional_u32(&mut digest, root.ty.map(|id| id.0));
            digest_optional_u32(&mut digest, root.namespace.map(|id| id.0));
            digest_bool(&mut digest, root.global_object_contributor);
            digest_bool(&mut digest, root.explicit_global_this);
        }
        digest_usize(&mut digest, self.owner_sites.len());
        for site in &self.owner_sites {
            digest_replay_owner(&mut digest, site.owner);
            digest_usize(&mut digest, site.file_ordinal.index());
            digest.update(site.span.start.to_le_bytes());
            digest.update(site.span.end.to_le_bytes());
            digest_site_provenance(&mut digest, &site.provenance);
        }
        digest_usize(&mut digest, self.reverse_edges.len());
        for edge in &self.reverse_edges {
            digest_replay_owner(&mut digest, edge.dependency);
            digest_replay_owner(&mut digest, edge.consumer);
        }
        digest_usize(&mut digest, self.root_slot_consumers.len());
        for consumer in &self.root_slot_consumers {
            digest_string(&mut digest, &consumer.name);
            digest.update([consumer.slot.tag()]);
            digest_replay_owner(&mut digest, consumer.consumer);
        }
        digest_usize(&mut digest, self.root_slot_owners.len());
        for owner in &self.root_slot_owners {
            digest_replay_owner(&mut digest, *owner);
        }
        digest_usize(&mut digest, self.namespace_direct_owners.len());
        for (namespace, owner) in &self.namespace_direct_owners {
            digest.update(namespace.0.to_le_bytes());
            digest_replay_owner(&mut digest, *owner);
        }
        digest_usize(&mut digest, self.statement_owners.len());
        for (key, owner) in &self.statement_owners {
            digest_library_event_key(&mut digest, *key);
            digest_replay_owner(&mut digest, *owner);
        }
        digest_usize(&mut digest, self.baseline_records.len());
        for record in &self.baseline_records {
            digest_replay_owner(&mut digest, record.owner);
            digest.update(record.record_count.to_le_bytes());
            digest.update(record.digest);
        }
        for boundary in &self.prefix_boundaries {
            digest_usize(&mut digest, boundary.cardinality);
            digest_bool(&mut digest, boundary.exact);
        }
        for value in [
            self.health.missing_active_owner_demands,
            self.health.unmatched_owner_sites,
            self.health.unowned_typed_references,
            self.health.raw_semantic_accesses,
            self.construction.library_source_compiles,
            self.construction.binder_source_censuses,
            self.construction.canonical_source_units,
            self.construction.second_source_censuses,
            self.construction.canonical_manifest_bytes,
            self.construction.rendered_record_digest_bytes,
            self.construction.transitive_terminal_owner_entries,
            self.construction.eager_all_owner_scc_memberships,
            self.construction.namespace_snapshot_rows,
            self.construction.runtime_snapshot_rows,
            self.construction.canonical_terminal_rows,
            self.construction.full_semantic_projection_rows,
            self.construction.ticket_slots,
            self.construction.ticket_owner_ordered_map_inserts,
            self.construction.owner_site_inner_heap_allocations,
            self.construction.owner_site_dense_slot_writes,
            self.construction.owner_site_ordered_map_inserts,
        ] {
            digest.update(value.to_le_bytes());
        }
        digest_bool(
            &mut digest,
            self.construction.trace_domain_sealed_after_binder_reporting,
        );
        digest.finalize().into()
    }
}

fn digest_bool(digest: &mut Sha256, value: bool) {
    digest.update([u8::from(value)]);
}

fn digest_usize(digest: &mut Sha256, value: usize) {
    digest.update(u64::try_from(value).unwrap_or(u64::MAX).to_le_bytes());
}

fn digest_string(digest: &mut Sha256, value: &str) {
    digest_usize(digest, value.len());
    digest.update(value.as_bytes());
}

fn digest_optional_u32(digest: &mut Sha256, value: Option<u32>) {
    digest_bool(digest, value.is_some());
    if let Some(value) = value {
        digest.update(value.to_le_bytes());
    }
}

fn digest_library_event_key(digest: &mut Sha256, key: LibraryEventKey) {
    digest_usize(digest, key.file_ordinal.index());
    digest.update(key.source_start.to_le_bytes());
    digest_usize(digest, key.event_ordinal);
    digest_usize(digest, key.record_ordinal);
}

fn digest_replay_owner(digest: &mut Sha256, owner: ReplayOwner) {
    digest.update([owner.tag()]);
    match owner {
        ReplayOwner::TypeGroup(id) => digest.update(id.0.to_le_bytes()),
        ReplayOwner::Value(id) => digest.update(id.0.to_le_bytes()),
        ReplayOwner::Namespace(id) => digest.update(id.0.to_le_bytes()),
        ReplayOwner::Class(id) => digest.update(id.0.to_le_bytes()),
        ReplayOwner::GlobalObject => {}
        ReplayOwner::Statement(key) => digest_library_event_key(digest, key),
    }
}

fn digest_declaration_owner(digest: &mut Sha256, owner: DeclarationOwner) {
    match owner {
        DeclarationOwner::Lexical(id) => {
            digest.update([0]);
            digest.update(id.0.to_le_bytes());
        }
        DeclarationOwner::NamespacePublic(id) => {
            digest.update([1]);
            digest.update(id.0.to_le_bytes());
        }
        DeclarationOwner::NamespacePrivate(id) => {
            digest.update([2]);
            digest.update(id.0.to_le_bytes());
        }
        DeclarationOwner::CompilationGlobal => digest.update([3]),
        DeclarationOwner::DeferredAmbientModule(id) => {
            digest.update([4]);
            digest.update(id.0.to_le_bytes());
        }
    }
}

fn digest_site_provenance(digest: &mut Sha256, provenance: &CollisionReplaySiteProvenance) {
    match provenance {
        CollisionReplaySiteProvenance::Declaration {
            declaration,
            kind,
            binder_owner,
            containing_namespace,
        } => {
            digest.update([0, declaration_kind_tag(*kind)]);
            digest.update(declaration.0.to_le_bytes());
            digest_declaration_owner(digest, *binder_owner);
            match containing_namespace {
                Some(namespace) => {
                    digest.update([1]);
                    digest.update(namespace.0.to_le_bytes());
                }
                None => digest.update([0]),
            }
        }
        CollisionReplaySiteProvenance::Event { phase } => {
            digest.update([1, event_phase_tag(*phase)]);
        }
        CollisionReplaySiteProvenance::GlobalContributor {
            name,
            kind,
            binder_owner,
        } => {
            digest.update([2, global_contributor_kind_tag(*kind)]);
            digest_string(digest, name);
            digest_declaration_owner(digest, *binder_owner);
        }
        CollisionReplaySiteProvenance::ExplicitGlobalThis { binder_owner } => {
            digest.update([3]);
            digest_declaration_owner(digest, *binder_owner);
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawSemanticAccessAuditForTest {
    pub source_manifest_complete: bool,
    pub injected_bypass_rejected: bool,
}

#[cfg(any(test, feature = "test-utils"))]
pub fn raw_semantic_access_audit_for_test() -> Result<RawSemanticAccessAuditForTest, String> {
    raw_access_audit::audit()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdmittedCollisionReplayIndex {
    pub schema: u32,
    pub owner_partition: Vec<ReplayOwner>,
    pub root_slots: Vec<ReplayRootSlot>,
    pub owner_sites: Vec<ReplayOwnerSite>,
    pub reverse_edges: Vec<ReplayReverseEdge>,
    pub root_slot_consumers: Vec<ReplayRootConsumer>,
    pub scc_membership: Vec<ReplayScc>,
    pub statement_owners: Vec<(LibraryEventKey, ReplayOwner)>,
    pub baseline_records: Vec<ReplayBaselineRecord>,
    pub unowned_demand_count: u64,
    pub invalid_owner_site_count: u64,
    pub noncanonical_edge_count: u64,
    pub typed_reference_coverage_misses: u64,
    pub owner_to_scc: Vec<u32>,
    pub scc_owner_ranges: Vec<ReplayRowRange>,
    pub scc_owners: Vec<ReplayOwner>,
    pub reverse_scc_offsets: Vec<u32>,
    pub reverse_scc_edges: Vec<u32>,
    pub root_slot_lookup: rustc_hash::FxHashMap<Box<str>, ReplayRootLookup>,
    pub owner_site_ranges: Vec<ReplayRowRange>,
    pub baseline_record_ranges: Vec<ReplayRowRange>,
    pub canonical_manifest_len: usize,
    pub canonical_manifest_sha256: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplayRowRange {
    start: u32,
    end: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayRootLookup {
    pub root_ordinal: u32,
    pub value_seeds: SmallVec<[ReplayOwner; 1]>,
    pub type_seeds: SmallVec<[ReplayOwner; 1]>,
    pub namespace_seeds: SmallVec<[ReplayOwner; 1]>,
}

impl ReplayRootLookup {
    fn seeds_mut(&mut self, slot: RootSlotKind) -> &mut SmallVec<[ReplayOwner; 1]> {
        match slot {
            RootSlotKind::Value => &mut self.value_seeds,
            RootSlotKind::Type => &mut self.type_seeds,
            RootSlotKind::Namespace => &mut self.namespace_seeds,
        }
    }
}

impl ReplayRowRange {
    fn checked(start: usize, end: usize) -> Result<Self, ReplayIndexAdmissionError> {
        Ok(Self {
            start: u32::try_from(start).map_err(|_| ReplayIndexAdmissionError::InvalidEncoding)?,
            end: u32::try_from(end).map_err(|_| ReplayIndexAdmissionError::InvalidEncoding)?,
        })
    }

    #[allow(dead_code)] // Used when the ADR-0015 collision route consumes admitted ranges.
    fn indices(self) -> Option<std::ops::Range<usize>> {
        Some(usize::try_from(self.start).ok()?..usize::try_from(self.end).ok()?)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Used by the pending ADR-0015 production collision route.
pub enum SparseReplayScheduleError {
    UnknownSeed(ReplayOwner),
    MissingScc(u32),
}

#[allow(dead_code)] // Used by the pending ADR-0015 production collision route.
pub trait SparseReplayGraphAccess {
    fn owner_scc(&self, owner: ReplayOwner) -> Option<u32>;
    fn reverse_sccs(&self, scc: u32) -> Option<&[u32]>;
    fn scc_owners(&self, scc: u32) -> Option<&[ReplayOwner]>;

    fn observe_seed_probe(&self, _owner: ReplayOwner) {}
    fn observe_queue_push(&self, _scc: u32) {}
    fn observe_queue_pop(&self, _scc: u32) {}
    fn observe_reverse_edge_probe(&self, _dependency: u32, _consumer: u32) {}
    fn observe_affected_scc_insert(&self, _scc: u32) {}
    fn observe_affected_owner_emission(&self, _owner: ReplayOwner) {}
}

#[allow(dead_code)] // Used by the pending ADR-0015 production collision route.
pub fn schedule_sparse_collision_closure<G: SparseReplayGraphAccess + ?Sized>(
    graph: &G,
    seeds: &[ReplayOwner],
) -> Result<Vec<ReplayOwner>, SparseReplayScheduleError> {
    let mut affected_sccs = rustc_hash::FxHashSet::default();
    let mut pending = Vec::new();
    for seed in seeds {
        graph.observe_seed_probe(*seed);
        let scc = graph
            .owner_scc(*seed)
            .ok_or(SparseReplayScheduleError::UnknownSeed(*seed))?;
        if affected_sccs.insert(scc) {
            graph.observe_affected_scc_insert(scc);
            graph.observe_queue_push(scc);
            pending.push(scc);
        }
    }

    let mut pending_cursor = 0;
    while let Some(dependency) = pending.get(pending_cursor).copied() {
        pending_cursor += 1;
        graph.observe_queue_pop(dependency);
        let consumers = graph
            .reverse_sccs(dependency)
            .ok_or(SparseReplayScheduleError::MissingScc(dependency))?;
        for consumer in consumers {
            graph.observe_reverse_edge_probe(dependency, *consumer);
            if affected_sccs.insert(*consumer) {
                graph.observe_affected_scc_insert(*consumer);
                graph.observe_queue_push(*consumer);
                pending.push(*consumer);
            }
        }
    }

    let mut affected_sccs = affected_sccs.into_iter().collect::<Vec<_>>();
    affected_sccs.sort_unstable();
    let mut affected_owners = Vec::new();
    for scc in affected_sccs {
        let owners = graph
            .scc_owners(scc)
            .ok_or(SparseReplayScheduleError::MissingScc(scc))?;
        for owner in owners {
            graph.observe_affected_owner_emission(*owner);
            affected_owners.push(*owner);
        }
    }
    Ok(affected_owners)
}

impl SparseReplayGraphAccess for AdmittedCollisionReplayIndex {
    fn owner_scc(&self, owner: ReplayOwner) -> Option<u32> {
        let ordinal = self.owner_partition.binary_search(&owner).ok()?;
        self.owner_to_scc.get(ordinal).copied()
    }

    fn reverse_sccs(&self, scc: u32) -> Option<&[u32]> {
        let scc = usize::try_from(scc).ok()?;
        let start = usize::try_from(*self.reverse_scc_offsets.get(scc)?).ok()?;
        let end = usize::try_from(*self.reverse_scc_offsets.get(scc.checked_add(1)?)?).ok()?;
        self.reverse_scc_edges.get(start..end)
    }

    fn scc_owners(&self, scc: u32) -> Option<&[ReplayOwner]> {
        let range = *self.scc_owner_ranges.get(usize::try_from(scc).ok()?)?;
        self.scc_owners.get(range.indices()?)
    }
}

impl AdmittedCollisionReplayIndex {
    #[cfg(any(test, feature = "test-utils"))]
    pub const fn canonical_manifest_len(&self) -> usize {
        self.canonical_manifest_len
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayIndexAdmissionError {
    InvalidEncoding,
    InvalidOwnerPartition,
    InvalidRootIndex,
    InvalidDependencyGraph,
    InvalidOwnerSites,
    InvalidSccPartition,
    InvalidStatementPartition,
    InvalidBaselinePartition,
    NonzeroGenerationHealthCounter,
    ManifestIdentityMismatch,
}

#[derive(Clone, Copy)]
pub struct ReplayIndexAdmissionLimits<'roots> {
    pub type_groups: usize,
    pub value_storages: usize,
    pub namespaces: usize,
    pub classes: usize,
    pub source_files: usize,
    pub roots: &'roots [ReplayRootIdentity],
}

pub type ReplayRootIdentity = (
    String,
    Option<ValueStorageId>,
    Option<TypeGroupId>,
    Option<NamespaceId>,
);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplayIndexGenerationError {
    UnownedDemands {
        count: u64,
        samples: Vec<ReplayUnownedDemandSample>,
    },
    InvalidOwnerSites(u64),
    NoncanonicalEdges(u64),
    TypedReferenceCoverage {
        count: u64,
        samples: Vec<ReplayCoverageMiss>,
    },
    MissingOwnerSite(ReplayOwner),
    UnknownOwner(ReplayOwner),
    DuplicateOwner(ReplayOwner),
    DuplicateOwnerSite(ReplayOwner),
    DuplicateRootName(String),
    InvalidRootSlot(String),
    DuplicateStatement(LibraryEventKey),
    StatementPartitionMismatch,
    DuplicateBaseline(ReplayOwner),
    BaselinePartitionMismatch,
    IndependentOwnerSiteOracleMismatch,
    ActiveOwnerScopes,
    SharedTraceAtFinalization,
    IntegerOverflow,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReplayUnownedDemandSample {
    pub dependency: ReplayDependencyKey,
    pub boundary: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReplayCoverageMiss {
    Boundary(&'static str),
    Dependency {
        consumer: ReplayOwner,
        dependency: ReplayOwner,
    },
}

struct ReplayTraceState {
    active_owners: Vec<ReplayOwner>,
    active_typed_observations: Vec<bool>,
    consumer_dependencies: BTreeMap<ReplayOwner, BTreeSet<ReplayDependencyKey>>,
    unowned_demand_count: u64,
    unowned_demand_samples: BTreeSet<ReplayUnownedDemandSample>,
    typed_reference_coverage_misses: u64,
    typed_reference_coverage_samples: BTreeSet<ReplayCoverageMiss>,
    raw_semantic_accesses: u64,
    owner_sites: ReplayOwnerSiteStorage,
    duplicate_owner_site_count: u64,
    missing_owner_site_count: u64,
    trace_domain_sealed_after_binder_reporting: bool,
}

impl ReplayTraceState {
    fn new(seed: ReplayTraceSeed) -> Self {
        Self {
            active_owners: Vec::new(),
            active_typed_observations: Vec::new(),
            consumer_dependencies: BTreeMap::new(),
            unowned_demand_count: 0,
            unowned_demand_samples: BTreeSet::new(),
            typed_reference_coverage_misses: 0,
            typed_reference_coverage_samples: BTreeSet::new(),
            raw_semantic_accesses: 0,
            owner_sites: seed.owner_sites,
            duplicate_owner_site_count: seed.duplicate_owner_site_count,
            missing_owner_site_count: seed.missing_owner_site_count,
            trace_domain_sealed_after_binder_reporting: seed
                .trace_domain_sealed_after_binder_reporting,
        }
    }
}

/// Pass-local, source-generation-only trace. Clones share one compiler-owned collector.
#[derive(Clone)]
pub struct ReplayDependencyTrace {
    state: Rc<RefCell<ReplayTraceState>>,
}

impl Default for ReplayDependencyTrace {
    fn default() -> Self {
        Self::new(ReplayTraceSeed::new(OwnerSiteStorageMode::Flat))
    }
}

impl ReplayDependencyTrace {
    pub fn scope(&self, owner: ReplayOwner) -> ReplayOwnerScope {
        self.enter(owner);
        ReplayOwnerScope {
            trace: self.clone(),
            owner,
        }
    }
    pub fn new(seed: ReplayTraceSeed) -> Self {
        Self {
            state: Rc::new(RefCell::new(ReplayTraceState::new(seed))),
        }
    }

    pub fn statement_owner(&self, ticket: (usize, usize)) -> Option<ReplayOwner> {
        self.state
            .borrow()
            .owner_sites
            .ticket_owner(ticket)
            .map(ReplayOwner::Statement)
    }

    pub fn statement_keys(&self) -> Vec<LibraryEventKey> {
        self.state.borrow().owner_sites.ticket_owners()
    }

    pub fn current_owner(&self) -> Option<ReplayOwner> {
        self.state.borrow().active_owners.last().copied()
    }

    pub fn remove_direct_dependencies(&self, edges: &BTreeSet<ReplayReverseEdge>) {
        let mut state = self.state.borrow_mut();
        for edge in edges {
            let Some(dependencies) = state.consumer_dependencies.get_mut(&edge.consumer) else {
                continue;
            };
            dependencies.remove(&ReplayDependencyKey::Owner(edge.dependency));
        }
    }

    pub fn enter(&self, owner: ReplayOwner) {
        self.state.borrow_mut().active_owners.push(owner);
    }

    pub fn record_owner_site(&self, site: CollisionReplayOwnerSite) {
        self.state.borrow_mut().owner_sites.record_direct(site);
    }

    pub fn leave(&self, expected: ReplayOwner) {
        let actual = self.state.borrow_mut().active_owners.pop();
        assert_eq!(
            actual,
            Some(expected),
            "replay owner scopes restore in LIFO order"
        );
    }

    pub fn demand(&self, dependency: ReplayOwner) {
        self.demand_at(dependency, "owner-demand");
    }

    pub fn demand_new(&self, dependency: ReplayOwner) -> bool {
        self.demand_key(ReplayDependencyKey::Owner(dependency), "owner-demand")
    }

    pub fn demand_at(&self, dependency: ReplayOwner, boundary: &'static str) {
        self.demand_key(ReplayDependencyKey::Owner(dependency), boundary);
    }

    pub fn demand_at_new(&self, dependency: ReplayOwner, boundary: &'static str) -> bool {
        self.demand_key(ReplayDependencyKey::Owner(dependency), boundary)
    }

    pub fn demand_root_slot(&self, name: &str, slot: RootSlotKind) {
        self.demand_key(
            ReplayDependencyKey::RootSlot {
                name: name.to_owned(),
                slot,
            },
            "root-slot",
        );
    }

    pub fn observe_typed_demand(&self, boundary: &'static str) -> ReplayTypedDemandObservation {
        self.state
            .borrow_mut()
            .active_typed_observations
            .push(false);
        ReplayTypedDemandObservation {
            trace: self.clone(),
            boundary,
        }
    }

    pub fn cover_typed_observations(&self) {
        for covered in &mut self.state.borrow_mut().active_typed_observations {
            *covered = true;
        }
    }

    pub fn record_raw_semantic_access(&self) {
        let mut state = self.state.borrow_mut();
        state.raw_semantic_accesses = state.raw_semantic_accesses.saturating_add(1);
    }

    pub fn record_statement_dependency(&self, ticket: (usize, usize), producer: ReplayOwner) {
        let mut state = self.state.borrow_mut();
        let Some(key) = state.owner_sites.ticket_owner(ticket) else {
            state.unowned_demand_count = state.unowned_demand_count.saturating_add(1);
            return;
        };
        let consumer = ReplayOwner::Statement(key);
        if consumer == producer {
            return;
        }
        state
            .consumer_dependencies
            .entry(consumer)
            .or_default()
            .insert(ReplayDependencyKey::Owner(producer));
    }

    pub fn require_dependency(&self, consumer: ReplayOwner, dependency: ReplayOwner) -> bool {
        if consumer == dependency {
            return true;
        }
        let mut state = self.state.borrow_mut();
        let mut pending = vec![consumer];
        let mut visited = BTreeSet::new();
        let mut present = false;
        while let Some(owner) = pending.pop() {
            if !visited.insert(owner) {
                continue;
            }
            for target in state
                .consumer_dependencies
                .get(&owner)
                .into_iter()
                .flatten()
            {
                let ReplayDependencyKey::Owner(target) = target else {
                    continue;
                };
                if *target == dependency {
                    present = true;
                    break;
                }
                pending.push(*target);
            }
            if present {
                break;
            }
        }
        if !present {
            state.typed_reference_coverage_misses =
                state.typed_reference_coverage_misses.saturating_add(1);
            if state.typed_reference_coverage_samples.len() < 16 {
                state
                    .typed_reference_coverage_samples
                    .insert(ReplayCoverageMiss::Dependency {
                        consumer,
                        dependency,
                    });
            }
        }
        present
    }

    fn demand_key(&self, dependency: ReplayDependencyKey, boundary: &'static str) -> bool {
        let mut state = self.state.borrow_mut();
        for covered in &mut state.active_typed_observations {
            *covered = true;
        }
        let Some(consumer) = state.active_owners.last().copied() else {
            state.unowned_demand_count = state.unowned_demand_count.saturating_add(1);
            if state.unowned_demand_samples.len() < 16 {
                state
                    .unowned_demand_samples
                    .insert(ReplayUnownedDemandSample {
                        dependency,
                        boundary,
                    });
            }
            return false;
        };
        if dependency == ReplayDependencyKey::Owner(consumer) {
            return false;
        }
        state
            .consumer_dependencies
            .entry(consumer)
            .or_default()
            .insert(dependency)
    }

    fn finish_typed_observation(&self, boundary: &'static str) {
        let mut state = self.state.borrow_mut();
        let covered = state
            .active_typed_observations
            .pop()
            .expect("typed replay observations restore in LIFO order");
        if !covered {
            state.typed_reference_coverage_misses =
                state.typed_reference_coverage_misses.saturating_add(1);
            if state.typed_reference_coverage_samples.len() < 16 {
                state
                    .typed_reference_coverage_samples
                    .insert(ReplayCoverageMiss::Boundary(boundary));
            }
        }
    }

    #[cfg(test)]
    fn counters(&self) -> (u64, u64) {
        let state = self.state.borrow();
        (state.unowned_demand_count, 0)
    }

    pub fn finish_compact_plan(
        self,
        mut root_slots: Vec<ReplayRootSlot>,
        baseline_records: Vec<ReplayBaselineRecord>,
        prefix_cardinalities: [usize; 9],
        mut construction: CollisionReplayConstructionEvidence,
    ) -> Result<CollisionReplayPlan, ReplayIndexGenerationError> {
        let state = Rc::try_unwrap(self.state)
            .map_err(|_| ReplayIndexGenerationError::SharedTraceAtFinalization)?
            .into_inner();
        if !state.active_owners.is_empty() || !state.active_typed_observations.is_empty() {
            return Err(ReplayIndexGenerationError::ActiveOwnerScopes);
        }
        if state.unowned_demand_count != 0 {
            return Err(ReplayIndexGenerationError::UnownedDemands {
                count: state.unowned_demand_count,
                samples: state.unowned_demand_samples.iter().cloned().collect(),
            });
        }
        if state.typed_reference_coverage_misses != 0 {
            return Err(ReplayIndexGenerationError::TypedReferenceCoverage {
                count: state.typed_reference_coverage_misses,
                samples: state
                    .typed_reference_coverage_samples
                    .iter()
                    .cloned()
                    .collect(),
            });
        }
        let invalid_owner_site_count = state
            .duplicate_owner_site_count
            .saturating_add(state.missing_owner_site_count);
        if invalid_owner_site_count != 0 {
            return Err(ReplayIndexGenerationError::InvalidOwnerSites(
                invalid_owner_site_count,
            ));
        }
        let health = CollisionReplayPlanHealth {
            missing_active_owner_demands: state.unowned_demand_count,
            unmatched_owner_sites: invalid_owner_site_count,
            unowned_typed_references: state.typed_reference_coverage_misses,
            raw_semantic_accesses: state.raw_semantic_accesses,
        };
        if health.raw_semantic_accesses != 0 {
            return Err(ReplayIndexGenerationError::TypedReferenceCoverage {
                count: health.raw_semantic_accesses,
                samples: vec![ReplayCoverageMiss::Boundary("raw-semantic-access")],
            });
        }

        let finished_storage = state.owner_sites.finish();
        let mut owner_sites = finished_storage.owner_sites;
        construction.ticket_slots = finished_storage.ticket_slots;
        construction.ticket_owner_ordered_map_inserts =
            finished_storage.ticket_owner_ordered_map_inserts;
        construction.owner_site_inner_heap_allocations =
            finished_storage.owner_site_inner_heap_allocations;
        construction.owner_site_dense_slot_writes = finished_storage.dense_slot_writes;
        construction.owner_site_ordered_map_inserts = finished_storage.ordered_map_inserts;
        construction.trace_domain_sealed_after_binder_reporting =
            state.trace_domain_sealed_after_binder_reporting;
        canonicalize_collision_replay_owner_sites(&mut owner_sites);
        let invalid_sites = owner_sites
            .iter()
            .filter(|site| site.span.start > site.span.end)
            .count();
        if invalid_sites != 0 {
            return Err(ReplayIndexGenerationError::InvalidOwnerSites(
                u64::try_from(invalid_sites)
                    .map_err(|_| ReplayIndexGenerationError::IntegerOverflow)?,
            ));
        }
        if owner_sites.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(ReplayIndexGenerationError::InvalidOwnerSites(1));
        }

        let mut statement_keys = finished_storage.ticket_owners;
        statement_keys.sort_unstable();
        if let Some(duplicate) = statement_keys
            .windows(2)
            .find(|pair| pair[0] == pair[1])
            .map(|pair| pair[0])
        {
            return Err(ReplayIndexGenerationError::DuplicateStatement(duplicate));
        }
        let mut baseline_records = baseline_records;
        baseline_records.sort_by_key(|record| record.owner);
        if let Some(duplicate) = baseline_records
            .windows(2)
            .find(|pair| pair[0].owner == pair[1].owner)
            .map(|pair| pair[0].owner)
        {
            return Err(ReplayIndexGenerationError::DuplicateBaseline(duplicate));
        }
        if baseline_records
            .iter()
            .any(|record| record.record_count == 0)
        {
            return Err(ReplayIndexGenerationError::BaselinePartitionMismatch);
        }

        let mut reverse_edges = Vec::new();
        let mut root_slot_consumers = Vec::new();
        for (consumer, dependencies) in state.consumer_dependencies {
            for dependency in dependencies {
                match dependency {
                    ReplayDependencyKey::Owner(dependency) => {
                        reverse_edges.push(ReplayReverseEdge {
                            dependency,
                            consumer,
                        });
                    }
                    ReplayDependencyKey::RootSlot { name, slot } => {
                        root_slot_consumers.push(ReplayRootConsumer {
                            name,
                            slot,
                            consumer,
                        });
                    }
                }
            }
        }
        reverse_edges.sort_unstable();
        root_slot_consumers.sort_unstable();
        let published_root_names = root_slots
            .iter()
            .map(|root| root.name.clone())
            .collect::<BTreeSet<_>>();
        for name in root_slot_consumers
            .iter()
            .map(|dependency| dependency.name.clone())
            .collect::<BTreeSet<_>>()
        {
            if !published_root_names.contains(&name) {
                root_slots.push(ReplayRootSlot {
                    name,
                    value: None,
                    ty: None,
                    namespace: None,
                    global_object_contributor: false,
                    explicit_global_this: false,
                });
            }
        }
        root_slots.sort_by(|left, right| left.name.cmp(&right.name));
        if let Some(duplicate) = root_slots
            .windows(2)
            .find(|pair| pair[0].name == pair[1].name)
            .map(|pair| pair[0].name.clone())
        {
            return Err(ReplayIndexGenerationError::DuplicateRootName(duplicate));
        }
        let mut dense_next = [0_u32; 4];
        let mut statement_site_keys = Vec::new();
        let mut global_owner_count = 0_u64;
        let mut last_owner = None;
        for site in &owner_sites {
            let provenance_matches = matches!(
                (&site.owner, &site.provenance),
                (
                    ReplayOwner::TypeGroup(_)
                        | ReplayOwner::Value(_)
                        | ReplayOwner::Namespace(_)
                        | ReplayOwner::Class(_),
                    CollisionReplaySiteProvenance::Declaration { .. }
                ) | (
                    ReplayOwner::Statement(_),
                    CollisionReplaySiteProvenance::Event { .. }
                ) | (
                    ReplayOwner::GlobalObject,
                    CollisionReplaySiteProvenance::GlobalContributor {
                        binder_owner: DeclarationOwner::CompilationGlobal,
                        ..
                    } | CollisionReplaySiteProvenance::ExplicitGlobalThis {
                        binder_owner: DeclarationOwner::CompilationGlobal,
                    }
                )
            );
            if !provenance_matches {
                return Err(ReplayIndexGenerationError::InvalidOwnerSites(1));
            }
            if last_owner == Some(site.owner) {
                continue;
            }
            last_owner = Some(site.owner);
            match site.owner {
                ReplayOwner::TypeGroup(id) => {
                    if id.0 != dense_next[0] {
                        return Err(ReplayIndexGenerationError::BaselinePartitionMismatch);
                    }
                    dense_next[0] = dense_next[0]
                        .checked_add(1)
                        .ok_or(ReplayIndexGenerationError::IntegerOverflow)?;
                }
                ReplayOwner::Value(id) => {
                    if id.0 != dense_next[1] {
                        return Err(ReplayIndexGenerationError::BaselinePartitionMismatch);
                    }
                    dense_next[1] = dense_next[1]
                        .checked_add(1)
                        .ok_or(ReplayIndexGenerationError::IntegerOverflow)?;
                }
                ReplayOwner::Namespace(id) => {
                    if id.0 != dense_next[2] {
                        return Err(ReplayIndexGenerationError::BaselinePartitionMismatch);
                    }
                    dense_next[2] = dense_next[2]
                        .checked_add(1)
                        .ok_or(ReplayIndexGenerationError::IntegerOverflow)?;
                }
                ReplayOwner::Class(id) => {
                    if id.0 != dense_next[3] {
                        return Err(ReplayIndexGenerationError::BaselinePartitionMismatch);
                    }
                    dense_next[3] = dense_next[3]
                        .checked_add(1)
                        .ok_or(ReplayIndexGenerationError::IntegerOverflow)?;
                }
                ReplayOwner::GlobalObject => {
                    global_owner_count = global_owner_count.saturating_add(1)
                }
                ReplayOwner::Statement(key) => statement_site_keys.push(key),
            }
        }
        let dense_domain_matches = [
            (dense_next[0], prefix_cardinalities[6]),
            (dense_next[1], prefix_cardinalities[8]),
            (dense_next[2], prefix_cardinalities[7]),
            (dense_next[3], prefix_cardinalities[2]),
        ]
        .into_iter()
        .all(|(observed, expected)| usize::try_from(observed) == Ok(expected));
        let needs_global_owner = root_slots
            .iter()
            .any(|root| root.global_object_contributor || root.explicit_global_this);
        if !dense_domain_matches
            || statement_site_keys != statement_keys
            || global_owner_count != u64::from(needs_global_owner)
        {
            return Err(ReplayIndexGenerationError::BaselinePartitionMismatch);
        }
        let has_owner_site = |owner: ReplayOwner| {
            owner_sites
                .binary_search_by_key(&owner, |site| site.owner)
                .is_ok()
        };
        if root_slots.iter().any(|root| {
            root.name.is_empty()
                || root
                    .value
                    .is_some_and(|id| !has_owner_site(ReplayOwner::Value(id)))
                || root
                    .ty
                    .is_some_and(|id| !has_owner_site(ReplayOwner::TypeGroup(id)))
                || root
                    .namespace
                    .is_some_and(|id| !has_owner_site(ReplayOwner::Namespace(id)))
        }) {
            return Err(ReplayIndexGenerationError::InvalidRootSlot(
                "<compact-plan>".to_owned(),
            ));
        }
        let noncanonical_edge_count = reverse_edges
            .iter()
            .filter(|edge| {
                edge.dependency == edge.consumer
                    || !has_owner_site(edge.dependency)
                    || !has_owner_site(edge.consumer)
            })
            .count()
            .saturating_add(
                root_slot_consumers
                    .iter()
                    .filter(|row| {
                        !has_owner_site(row.consumer)
                            || root_slots
                                .binary_search_by(|root| root.name.cmp(&row.name))
                                .is_err()
                    })
                    .count(),
            )
            .saturating_add(
                baseline_records
                    .iter()
                    .filter(|record| {
                        !matches!(record.owner, ReplayOwner::Statement(_))
                            || !has_owner_site(record.owner)
                    })
                    .count(),
            );
        if noncanonical_edge_count != 0 {
            return Err(ReplayIndexGenerationError::NoncanonicalEdges(
                u64::try_from(noncanonical_edge_count)
                    .map_err(|_| ReplayIndexGenerationError::IntegerOverflow)?,
            ));
        }
        let mut root_slot_owners = root_slots
            .iter()
            .flat_map(|root| {
                [
                    root.value.map(ReplayOwner::Value),
                    root.ty.map(ReplayOwner::TypeGroup),
                    root.namespace.map(ReplayOwner::Namespace),
                ]
                .into_iter()
                .flatten()
            })
            .collect::<Vec<_>>();
        root_slot_owners.sort_unstable();
        root_slot_owners.dedup();
        let mut namespace_direct_owners = owner_sites
            .iter()
            .filter_map(|site| match site.provenance {
                CollisionReplaySiteProvenance::Declaration {
                    containing_namespace: Some(namespace),
                    ..
                } => Some((namespace, site.owner)),
                CollisionReplaySiteProvenance::Declaration {
                    containing_namespace: None,
                    ..
                }
                | CollisionReplaySiteProvenance::Event { .. }
                | CollisionReplaySiteProvenance::GlobalContributor { .. }
                | CollisionReplaySiteProvenance::ExplicitGlobalThis { .. } => None,
            })
            .collect::<Vec<_>>();
        namespace_direct_owners.sort_unstable();
        namespace_direct_owners.dedup();
        Ok(CollisionReplayPlan {
            root_names: published_root_names.into_iter().collect(),
            root_slots,
            owner_sites,
            reverse_edges,
            root_slot_consumers,
            root_slot_owners,
            namespace_direct_owners,
            statement_owners: statement_keys
                .into_iter()
                .map(|key| (key, ReplayOwner::Statement(key)))
                .collect(),
            baseline_records,
            prefix_boundaries: prefix_cardinalities.map(|cardinality| {
                CollisionReplayPrefixBoundary {
                    cardinality,
                    exact: false,
                }
            }),
            health,
            construction,
            sealed_digest: None,
        })
    }

    pub fn finish(
        self,
        owner_partition: Vec<ReplayOwner>,
        mut root_slots: Vec<ReplayRootSlot>,
        owner_sites: Vec<ReplayOwnerSite>,
        baseline_records: Vec<ReplayBaselineRecord>,
        invalid_owner_site_count: u64,
    ) -> Result<CollisionReplayIndex, ReplayIndexGenerationError> {
        let state = Rc::try_unwrap(self.state)
            .map_err(|_| ReplayIndexGenerationError::SharedTraceAtFinalization)?
            .into_inner();
        if !state.active_owners.is_empty() {
            return Err(ReplayIndexGenerationError::ActiveOwnerScopes);
        }
        if state.unowned_demand_count != 0 {
            return Err(ReplayIndexGenerationError::UnownedDemands {
                count: state.unowned_demand_count,
                samples: state.unowned_demand_samples.into_iter().collect(),
            });
        }
        if !state.active_typed_observations.is_empty() {
            return Err(ReplayIndexGenerationError::ActiveOwnerScopes);
        }
        if state.typed_reference_coverage_misses != 0 {
            return Err(ReplayIndexGenerationError::TypedReferenceCoverage {
                count: state.typed_reference_coverage_misses,
                samples: state.typed_reference_coverage_samples.into_iter().collect(),
            });
        }
        let mut statement_keys = state.owner_sites.finish().ticket_owners;

        let mut owner_partition = owner_partition;
        owner_partition.sort_unstable();
        if let Some(duplicate) = owner_partition
            .windows(2)
            .find(|pair| pair[0] == pair[1])
            .map(|pair| pair[0])
        {
            return Err(ReplayIndexGenerationError::DuplicateOwner(duplicate));
        }
        let partition = owner_partition.iter().copied().collect::<BTreeSet<_>>();

        let mut owner_sites = owner_sites;
        owner_sites.sort_by_key(|site| {
            (
                site.owner,
                site.file_ordinal,
                site.span.start,
                site.span.end,
            )
        });
        let invalid_sites = owner_sites
            .iter()
            .filter(|site| !partition.contains(&site.owner) || site.span.start > site.span.end)
            .count();
        let invalid_sites = invalid_owner_site_count.saturating_add(
            u64::try_from(invalid_sites)
                .map_err(|_| ReplayIndexGenerationError::IntegerOverflow)?,
        );
        if invalid_sites != 0 {
            return Err(ReplayIndexGenerationError::InvalidOwnerSites(invalid_sites));
        }
        if let Some(duplicate) = owner_sites
            .windows(2)
            .find(|pair| pair[0] == pair[1])
            .map(|pair| pair[0].owner)
        {
            return Err(ReplayIndexGenerationError::DuplicateOwnerSite(duplicate));
        }
        let site_owners = owner_sites
            .iter()
            .map(|site| site.owner)
            .collect::<BTreeSet<_>>();
        for owner in &owner_partition {
            if !site_owners.contains(owner) {
                return Err(ReplayIndexGenerationError::MissingOwnerSite(*owner));
            }
        }

        statement_keys.sort_unstable();
        if let Some(duplicate) = statement_keys
            .windows(2)
            .find(|pair| pair[0] == pair[1])
            .map(|pair| pair[0])
        {
            return Err(ReplayIndexGenerationError::DuplicateStatement(duplicate));
        }
        let statement_partition = owner_partition
            .iter()
            .filter_map(|owner| match owner {
                ReplayOwner::Statement(key) => Some(*key),
                _ => None,
            })
            .collect::<Vec<_>>();
        if statement_partition != statement_keys {
            return Err(ReplayIndexGenerationError::StatementPartitionMismatch);
        }

        let mut baseline_records = baseline_records;
        baseline_records.sort_by_key(|record| record.owner);
        if let Some(duplicate) = baseline_records
            .windows(2)
            .find(|pair| pair[0].owner == pair[1].owner)
            .map(|pair| pair[0].owner)
        {
            return Err(ReplayIndexGenerationError::DuplicateBaseline(duplicate));
        }
        if baseline_records
            .iter()
            .map(|record| record.owner)
            .ne(owner_partition.iter().copied())
        {
            return Err(ReplayIndexGenerationError::BaselinePartitionMismatch);
        }

        let mut reverse_edges = Vec::new();
        let mut root_slot_consumers = Vec::new();
        let mut graph = owner_partition
            .iter()
            .copied()
            .map(|owner| (owner, BTreeSet::new()))
            .collect::<BTreeMap<_, _>>();
        for (consumer, dependencies) in state.consumer_dependencies {
            if !partition.contains(&consumer) {
                return Err(ReplayIndexGenerationError::UnknownOwner(consumer));
            }
            for dependency in dependencies {
                match dependency {
                    ReplayDependencyKey::Owner(dependency) => {
                        if !partition.contains(&dependency) {
                            return Err(ReplayIndexGenerationError::UnknownOwner(dependency));
                        }
                        graph.entry(consumer).or_default().insert(dependency);
                        reverse_edges.push(ReplayReverseEdge {
                            dependency,
                            consumer,
                        });
                    }
                    ReplayDependencyKey::RootSlot { name, slot } => {
                        root_slot_consumers.push(ReplayRootConsumer {
                            name,
                            slot,
                            consumer,
                        });
                    }
                }
            }
        }
        reverse_edges.sort_unstable();
        root_slot_consumers.sort_unstable();
        let published_root_names = root_slots
            .iter()
            .map(|root| root.name.clone())
            .collect::<BTreeSet<_>>();
        for name in root_slot_consumers
            .iter()
            .map(|dependency| dependency.name.clone())
            .collect::<BTreeSet<_>>()
        {
            if !published_root_names.contains(&name) {
                root_slots.push(ReplayRootSlot {
                    name,
                    value: None,
                    ty: None,
                    namespace: None,
                    global_object_contributor: false,
                    explicit_global_this: false,
                });
            }
        }
        root_slots.sort_by(|left, right| left.name.cmp(&right.name));
        if let Some(duplicate) = root_slots
            .windows(2)
            .find(|pair| pair[0].name == pair[1].name)
            .map(|pair| pair[0].name.clone())
        {
            return Err(ReplayIndexGenerationError::DuplicateRootName(duplicate));
        }
        for root in &root_slots {
            let valid = !root.name.is_empty()
                && root
                    .value
                    .is_none_or(|id| partition.contains(&ReplayOwner::Value(id)))
                && root
                    .ty
                    .is_none_or(|id| partition.contains(&ReplayOwner::TypeGroup(id)))
                && root
                    .namespace
                    .is_none_or(|id| partition.contains(&ReplayOwner::Namespace(id)));
            if !valid {
                return Err(ReplayIndexGenerationError::InvalidRootSlot(
                    root.name.clone(),
                ));
            }
        }

        let noncanonical_edge_count =
            validate_edge_rows(&partition, &reverse_edges, &root_slot_consumers);
        if noncanonical_edge_count != 0 {
            return Err(ReplayIndexGenerationError::NoncanonicalEdges(
                noncanonical_edge_count,
            ));
        }

        let scc_membership = dependency_first_sccs(&graph)
            .into_iter()
            .enumerate()
            .map(|(index, owners)| {
                Ok(ReplayScc {
                    replay_ordinal: u32::try_from(index)
                        .map_err(|_| ReplayIndexGenerationError::IntegerOverflow)?,
                    owners: owners.into_iter().collect(),
                })
            })
            .collect::<Result<Vec<_>, ReplayIndexGenerationError>>()?;
        let statement_owners = statement_keys
            .into_iter()
            .map(|key| (key, ReplayOwner::Statement(key)))
            .collect::<Vec<_>>();
        let mut index = CollisionReplayIndex {
            schema: 1,
            owner_partition,
            root_slots,
            owner_sites,
            reverse_edges,
            root_slot_consumers,
            scc_membership,
            statement_owners,
            baseline_records,
            unowned_demand_count: 0,
            invalid_owner_site_count: 0,
            noncanonical_edge_count,
            typed_reference_coverage_misses: 0,
            canonical_manifest_bytes: Vec::new(),
            canonical_manifest_sha256: [0; 32],
        };
        index.canonical_manifest_bytes = index.encode()?;
        super::library_compiler::record_collision_manifest_bytes(
            index.canonical_manifest_bytes.len(),
        );
        index.canonical_manifest_sha256 = Sha256::digest(&index.canonical_manifest_bytes).into();
        Ok(index)
    }
}

fn validate_edge_rows(
    partition: &BTreeSet<ReplayOwner>,
    edges: &[ReplayReverseEdge],
    root_consumers: &[ReplayRootConsumer],
) -> u64 {
    let malformed_edges = edges
        .iter()
        .enumerate()
        .filter(|(index, edge)| {
            edge.dependency == edge.consumer
                || !partition.contains(&edge.dependency)
                || !partition.contains(&edge.consumer)
                || index
                    .checked_sub(1)
                    .is_some_and(|previous| edges[previous] >= **edge)
        })
        .count();
    let malformed_roots = root_consumers
        .iter()
        .enumerate()
        .filter(|(index, dependency)| {
            dependency.name.is_empty()
                || !partition.contains(&dependency.consumer)
                || index
                    .checked_sub(1)
                    .is_some_and(|previous| root_consumers[previous] >= **dependency)
        })
        .count();
    u64::try_from(malformed_edges.saturating_add(malformed_roots)).unwrap_or(u64::MAX)
}

pub struct ReplayClassLookup<'a> {
    published: &'a PublishedClasses,
    trace: Option<ReplayDependencyTrace>,
}

impl<'a> ReplayClassLookup<'a> {
    pub fn new(published: &'a PublishedClasses, trace: Option<ReplayDependencyTrace>) -> Self {
        Self { published, trace }
    }
}

impl PublishedClassLookup for ReplayClassLookup<'_> {
    fn published_class(&self, class: ClassId) -> DemandOutcome<&PublishedClassSurface> {
        self.observe_class_demand(class);
        self.published.published_class(class)
    }

    fn publication_identity(&self) -> &Arc<()> {
        self.published.identity()
    }

    fn require_class(&self, class: ClassId) -> DemandOutcome<()> {
        self.observe_class_demand(class);
        self.published.require(class)
    }

    fn observe_class_demand(&self, class: ClassId) {
        if let Some(trace) = &self.trace {
            let _observation = trace.observe_typed_demand("semantic-query-class-terminal");
            trace.demand_at(ReplayOwner::Class(class), "semantic-query-class-terminal");
        }
    }

    fn class_demand_observation_enabled(&self) -> bool {
        self.trace.is_some()
    }
}

pub struct ReplayOwnerScope {
    trace: ReplayDependencyTrace,
    owner: ReplayOwner,
}

pub struct ReplayTypedDemandObservation {
    trace: ReplayDependencyTrace,
    boundary: &'static str,
}

impl Drop for ReplayTypedDemandObservation {
    fn drop(&mut self) {
        self.trace.finish_typed_observation(self.boundary);
    }
}

impl Drop for ReplayOwnerScope {
    fn drop(&mut self) {
        self.trace.leave(self.owner);
    }
}

impl CollisionReplayIndex {
    pub fn encode(&self) -> Result<Vec<u8>, ReplayIndexGenerationError> {
        let mut bytes = ManifestBytes::new(COLLISION_REPLAY_MANIFEST_DOMAIN);
        bytes.u32(self.schema);
        bytes.usize(self.owner_partition.len())?;
        for owner in &self.owner_partition {
            owner.encode(&mut bytes)?;
        }
        bytes.usize(self.root_slots.len())?;
        for root in &self.root_slots {
            bytes.string(&root.name)?;
            bytes.optional_u32(root.value.map(|id| id.0));
            bytes.optional_u32(root.ty.map(|id| id.0));
            bytes.optional_u32(root.namespace.map(|id| id.0));
            bytes.u8(u8::from(root.global_object_contributor));
            bytes.u8(u8::from(root.explicit_global_this));
        }
        bytes.usize(self.owner_sites.len())?;
        for site in &self.owner_sites {
            site.owner.encode(&mut bytes)?;
            bytes.usize(site.file_ordinal.index())?;
            bytes.u32(site.span.start);
            bytes.u32(site.span.end);
        }
        bytes.usize(self.reverse_edges.len())?;
        for edge in &self.reverse_edges {
            edge.dependency.encode(&mut bytes)?;
            edge.consumer.encode(&mut bytes)?;
        }
        bytes.usize(self.root_slot_consumers.len())?;
        for dependency in &self.root_slot_consumers {
            bytes.string(&dependency.name)?;
            bytes.u8(dependency.slot.tag());
            dependency.consumer.encode(&mut bytes)?;
        }
        bytes.usize(self.scc_membership.len())?;
        for component in &self.scc_membership {
            bytes.u32(component.replay_ordinal);
            bytes.usize(component.owners.len())?;
            for owner in &component.owners {
                owner.encode(&mut bytes)?;
            }
        }
        bytes.usize(self.statement_owners.len())?;
        for (key, owner) in &self.statement_owners {
            ReplayOwner::Statement(*key).encode(&mut bytes)?;
            owner.encode(&mut bytes)?;
        }
        bytes.usize(self.baseline_records.len())?;
        for baseline in &self.baseline_records {
            baseline.owner.encode(&mut bytes)?;
            bytes.u64(baseline.record_count);
            bytes.raw(&baseline.digest);
        }
        bytes.u64(self.unowned_demand_count);
        bytes.u64(self.invalid_owner_site_count);
        bytes.u64(self.noncanonical_edge_count);
        bytes.u64(self.typed_reference_coverage_misses);
        Ok(bytes.finish())
    }
}

fn owner_in_bounds(owner: ReplayOwner, limits: ReplayIndexAdmissionLimits<'_>) -> bool {
    match owner {
        ReplayOwner::TypeGroup(id) => usize::try_from(id.0).is_ok_and(|id| id < limits.type_groups),
        ReplayOwner::Value(id) => usize::try_from(id.0).is_ok_and(|id| id < limits.value_storages),
        ReplayOwner::Namespace(id) => usize::try_from(id.0).is_ok_and(|id| id < limits.namespaces),
        ReplayOwner::Class(id) => usize::try_from(id.0).is_ok_and(|id| id < limits.classes),
        ReplayOwner::GlobalObject => true,
        ReplayOwner::Statement(key) => key.file_ordinal.index() < limits.source_files,
    }
}

/// Admit the index the source compiler just generated.
///
/// Admission is the single construction path: it re-checks the generator's structural
/// guarantees and derives the compact runtime indexes the collision scheduler walks.
pub fn admit_generated_collision_replay_index(
    decoded: CollisionReplayIndex,
    limits: ReplayIndexAdmissionLimits<'_>,
    expected_manifest_sha256: Option<[u8; 32]>,
) -> Result<AdmittedCollisionReplayIndex, ReplayIndexAdmissionError> {
    let canonical_manifest_len = decoded.canonical_manifest_bytes.len();
    let expected_nonstatement_count = limits
        .type_groups
        .checked_add(limits.value_storages)
        .and_then(|count| count.checked_add(limits.namespaces))
        .and_then(|count| count.checked_add(limits.classes))
        .and_then(|count| count.checked_add(1))
        .ok_or(ReplayIndexAdmissionError::InvalidOwnerPartition)?;
    if decoded.owner_partition.len() < expected_nonstatement_count {
        return Err(ReplayIndexAdmissionError::InvalidOwnerPartition);
    }
    let mut expected_nonstatement = Vec::with_capacity(expected_nonstatement_count);
    for id in 0..limits.type_groups {
        expected_nonstatement.push(ReplayOwner::TypeGroup(TypeGroupId(
            u32::try_from(id).map_err(|_| ReplayIndexAdmissionError::InvalidOwnerPartition)?,
        )));
    }
    for id in 0..limits.value_storages {
        expected_nonstatement.push(ReplayOwner::Value(ValueStorageId(
            u32::try_from(id).map_err(|_| ReplayIndexAdmissionError::InvalidOwnerPartition)?,
        )));
    }
    for id in 0..limits.namespaces {
        expected_nonstatement.push(ReplayOwner::Namespace(NamespaceId(
            u32::try_from(id).map_err(|_| ReplayIndexAdmissionError::InvalidOwnerPartition)?,
        )));
    }
    for id in 0..limits.classes {
        expected_nonstatement.push(ReplayOwner::Class(ClassId(
            u32::try_from(id).map_err(|_| ReplayIndexAdmissionError::InvalidOwnerPartition)?,
        )));
    }
    expected_nonstatement.push(ReplayOwner::GlobalObject);
    if decoded.owner_partition[..expected_nonstatement_count] != expected_nonstatement
        || decoded
            .owner_partition
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || decoded
            .owner_partition
            .iter()
            .copied()
            .any(|owner| !owner_in_bounds(owner, limits))
    {
        return Err(ReplayIndexAdmissionError::InvalidOwnerPartition);
    }
    let owner_ordinals = decoded
        .owner_partition
        .iter()
        .copied()
        .enumerate()
        .map(|(ordinal, owner)| (owner, ordinal))
        .collect::<rustc_hash::FxHashMap<_, _>>();

    if decoded.root_slots.iter().any(|root| root.name.is_empty())
        || decoded
            .root_slots
            .windows(2)
            .any(|pair| pair[0].name >= pair[1].name)
        || decoded.root_slots.iter().any(|root| {
            !root
                .value
                .is_none_or(|id| usize::try_from(id.0).is_ok_and(|id| id < limits.value_storages))
                || !root
                    .ty
                    .is_none_or(|id| usize::try_from(id.0).is_ok_and(|id| id < limits.type_groups))
                || !root
                    .namespace
                    .is_none_or(|id| usize::try_from(id.0).is_ok_and(|id| id < limits.namespaces))
        })
    {
        return Err(ReplayIndexAdmissionError::InvalidRootIndex);
    }
    let mut owner_site_ranges = Vec::with_capacity(decoded.owner_partition.len());
    let mut site_index = 0;
    let mut previous_site_key = None;
    for owner in &decoded.owner_partition {
        let range_start = site_index;
        while let Some(site) = decoded.owner_sites.get(site_index) {
            if site.owner != *owner {
                break;
            }
            let key = (
                site.owner,
                site.file_ordinal,
                site.span.start,
                site.span.end,
            );
            if previous_site_key.is_some_and(|previous| previous >= key)
                || site.file_ordinal.index() >= limits.source_files
                || site.span.start > site.span.end
            {
                return Err(ReplayIndexAdmissionError::InvalidOwnerSites);
            }
            previous_site_key = Some(key);
            site_index += 1;
        }
        if site_index == range_start {
            return Err(ReplayIndexAdmissionError::InvalidOwnerSites);
        }
        owner_site_ranges.push(
            ReplayRowRange::checked(range_start, site_index)
                .map_err(|_| ReplayIndexAdmissionError::InvalidOwnerSites)?,
        );
    }
    if site_index != decoded.owner_sites.len() {
        return Err(ReplayIndexAdmissionError::InvalidOwnerSites);
    }

    let root_names = decoded
        .root_slots
        .iter()
        .map(|root| root.name.as_str())
        .collect::<rustc_hash::FxHashSet<_>>();
    if decoded
        .reverse_edges
        .windows(2)
        .any(|pair| pair[0] >= pair[1])
        || decoded.reverse_edges.iter().any(|edge| {
            edge.dependency == edge.consumer
                || !owner_ordinals.contains_key(&edge.dependency)
                || !owner_ordinals.contains_key(&edge.consumer)
        })
        || decoded
            .root_slot_consumers
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || decoded.root_slot_consumers.iter().any(|edge| {
            !owner_ordinals.contains_key(&edge.consumer) || !root_names.contains(edge.name.as_str())
        })
    {
        return Err(ReplayIndexAdmissionError::InvalidDependencyGraph);
    }
    let canonical_roots = limits
        .roots
        .iter()
        .map(|root| (root.0.as_str(), (root.1, root.2, root.3)))
        .collect::<BTreeMap<_, _>>();
    let populated_roots = decoded
        .root_slots
        .iter()
        .filter(|root| root.value.is_some() || root.ty.is_some() || root.namespace.is_some())
        .map(|root| (root.name.as_str(), (root.value, root.ty, root.namespace)))
        .collect::<BTreeMap<_, _>>();
    let placeholder_names = decoded
        .root_slots
        .iter()
        .filter(|root| root.value.is_none() && root.ty.is_none() && root.namespace.is_none())
        .map(|root| root.name.as_str())
        .collect::<BTreeSet<_>>();
    let consumed_placeholder_names = decoded
        .root_slot_consumers
        .iter()
        .map(|consumer| consumer.name.as_str())
        .filter(|name| !canonical_roots.contains_key(name))
        .collect::<BTreeSet<_>>();
    if populated_roots != canonical_roots || placeholder_names != consumed_placeholder_names {
        return Err(ReplayIndexAdmissionError::InvalidRootIndex);
    }
    let mut root_slot_lookup = rustc_hash::FxHashMap::default();
    for (ordinal, root) in decoded.root_slots.iter().enumerate() {
        let lookup = ReplayRootLookup {
            root_ordinal: u32::try_from(ordinal)
                .map_err(|_| ReplayIndexAdmissionError::InvalidRootIndex)?,
            value_seeds: root.value.map(ReplayOwner::Value).into_iter().collect(),
            type_seeds: root.ty.map(ReplayOwner::TypeGroup).into_iter().collect(),
            namespace_seeds: root
                .namespace
                .map(ReplayOwner::Namespace)
                .into_iter()
                .collect(),
        };
        if root_slot_lookup
            .insert(root.name.clone().into_boxed_str(), lookup)
            .is_some()
        {
            return Err(ReplayIndexAdmissionError::InvalidRootIndex);
        }
    }
    for consumer in &decoded.root_slot_consumers {
        let lookup = root_slot_lookup
            .get_mut(consumer.name.as_str())
            .ok_or(ReplayIndexAdmissionError::InvalidRootIndex)?;
        lookup.seeds_mut(consumer.slot).push(consumer.consumer);
    }
    for lookup in root_slot_lookup.values_mut() {
        for owners in [
            &mut lookup.value_seeds,
            &mut lookup.type_seeds,
            &mut lookup.namespace_seeds,
        ] {
            owners.sort_unstable();
            owners.dedup();
        }
    }

    let mut owner_to_scc = vec![u32::MAX; decoded.owner_partition.len()];
    let mut scc_owner_ranges = Vec::with_capacity(decoded.scc_membership.len());
    let mut scc_owners = Vec::with_capacity(decoded.owner_partition.len());
    for (ordinal, component) in decoded.scc_membership.iter().enumerate() {
        let replay_ordinal =
            u32::try_from(ordinal).map_err(|_| ReplayIndexAdmissionError::InvalidSccPartition)?;
        if component.replay_ordinal != replay_ordinal
            || component.owners.is_empty()
            || component.owners.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(ReplayIndexAdmissionError::InvalidSccPartition);
        }
        let range_start = scc_owners.len();
        for owner in &component.owners {
            let Some(owner_ordinal) = owner_ordinals.get(owner).copied() else {
                return Err(ReplayIndexAdmissionError::InvalidSccPartition);
            };
            if owner_to_scc[owner_ordinal] != u32::MAX {
                return Err(ReplayIndexAdmissionError::InvalidSccPartition);
            }
            owner_to_scc[owner_ordinal] = replay_ordinal;
            scc_owners.push(*owner);
        }
        scc_owner_ranges.push(
            ReplayRowRange::checked(range_start, scc_owners.len())
                .map_err(|_| ReplayIndexAdmissionError::InvalidSccPartition)?,
        );
    }
    if owner_to_scc.contains(&u32::MAX) {
        return Err(ReplayIndexAdmissionError::InvalidSccPartition);
    }
    let mut forward = rustc_hash::FxHashMap::<ReplayOwner, Vec<ReplayOwner>>::default();
    let mut reverse = rustc_hash::FxHashMap::<ReplayOwner, Vec<ReplayOwner>>::default();
    let mut component_edges = Vec::new();
    for edge in &decoded.reverse_edges {
        let dependency_ordinal = owner_ordinals[&edge.dependency];
        let consumer_ordinal = owner_ordinals[&edge.consumer];
        let dependency_scc = owner_to_scc[dependency_ordinal];
        let consumer_scc = owner_to_scc[consumer_ordinal];
        if dependency_scc > consumer_scc {
            return Err(ReplayIndexAdmissionError::InvalidSccPartition);
        }
        if dependency_scc == consumer_scc {
            forward
                .entry(edge.consumer)
                .or_default()
                .push(edge.dependency);
            reverse
                .entry(edge.dependency)
                .or_default()
                .push(edge.consumer);
        } else {
            component_edges.push((dependency_scc, consumer_scc));
        }
    }
    drop(owner_ordinals);
    component_edges.sort_unstable();
    component_edges.dedup();

    let component_count = decoded.scc_membership.len();
    let mut dependency_counts = vec![0usize; component_count];
    let mut reverse_scc_offsets = Vec::with_capacity(component_count.saturating_add(1));
    let mut reverse_scc_edges = Vec::with_capacity(component_edges.len());
    let mut edge_index = 0;
    for dependency in 0..component_count {
        reverse_scc_offsets.push(
            u32::try_from(reverse_scc_edges.len())
                .map_err(|_| ReplayIndexAdmissionError::InvalidSccPartition)?,
        );
        let dependency = u32::try_from(dependency)
            .map_err(|_| ReplayIndexAdmissionError::InvalidSccPartition)?;
        while component_edges
            .get(edge_index)
            .is_some_and(|edge| edge.0 == dependency)
        {
            let consumer = component_edges[edge_index].1;
            let consumer_index = usize::try_from(consumer)
                .map_err(|_| ReplayIndexAdmissionError::InvalidSccPartition)?;
            dependency_counts[consumer_index] = dependency_counts[consumer_index]
                .checked_add(1)
                .ok_or(ReplayIndexAdmissionError::InvalidSccPartition)?;
            reverse_scc_edges.push(consumer);
            edge_index += 1;
        }
    }
    reverse_scc_offsets.push(
        u32::try_from(reverse_scc_edges.len())
            .map_err(|_| ReplayIndexAdmissionError::InvalidSccPartition)?,
    );
    if edge_index != component_edges.len() {
        return Err(ReplayIndexAdmissionError::InvalidSccPartition);
    }
    fn reaches_component(
        start: ReplayOwner,
        adjacency: &rustc_hash::FxHashMap<ReplayOwner, Vec<ReplayOwner>>,
    ) -> rustc_hash::FxHashSet<ReplayOwner> {
        let mut reached = rustc_hash::FxHashSet::default();
        let mut pending = vec![start];
        while let Some(owner) = pending.pop() {
            if reached.insert(owner) {
                pending.extend(adjacency.get(&owner).into_iter().flatten().copied());
            }
        }
        reached
    }
    for component in decoded
        .scc_membership
        .iter()
        .filter(|component| component.owners.len() > 1)
    {
        let members = component
            .owners
            .iter()
            .copied()
            .collect::<rustc_hash::FxHashSet<_>>();
        let start = component.owners[0];
        if reaches_component(start, &forward) != members
            || reaches_component(start, &reverse) != members
        {
            return Err(ReplayIndexAdmissionError::InvalidSccPartition);
        }
    }
    let mut ready = std::collections::BinaryHeap::new();
    for (component, count) in dependency_counts.iter().copied().enumerate() {
        if count == 0 {
            ready.push(std::cmp::Reverse((
                decoded.scc_membership[component].owners[0],
                component,
            )));
        }
    }
    for expected in 0..decoded.scc_membership.len() {
        let Some(std::cmp::Reverse((_, component))) = ready.pop() else {
            return Err(ReplayIndexAdmissionError::InvalidSccPartition);
        };
        if component != expected {
            return Err(ReplayIndexAdmissionError::InvalidSccPartition);
        }
        let edge_start = usize::try_from(reverse_scc_offsets[component])
            .map_err(|_| ReplayIndexAdmissionError::InvalidSccPartition)?;
        let edge_end = usize::try_from(reverse_scc_offsets[component + 1])
            .map_err(|_| ReplayIndexAdmissionError::InvalidSccPartition)?;
        for dependent in reverse_scc_edges[edge_start..edge_end].iter().copied() {
            let dependent = usize::try_from(dependent)
                .map_err(|_| ReplayIndexAdmissionError::InvalidSccPartition)?;
            dependency_counts[dependent] -= 1;
            if dependency_counts[dependent] == 0 {
                ready.push(std::cmp::Reverse((
                    decoded.scc_membership[dependent].owners[0],
                    dependent,
                )));
            }
        }
    }
    if !ready.is_empty() {
        return Err(ReplayIndexAdmissionError::InvalidSccPartition);
    }

    let expected_statements = decoded.owner_partition[expected_nonstatement_count..]
        .iter()
        .filter_map(|owner| match owner {
            ReplayOwner::Statement(key) => Some((*key, *owner)),
            _ => None,
        })
        .collect::<Vec<_>>();
    if expected_statements.len() != decoded.owner_partition.len() - expected_nonstatement_count
        || decoded.statement_owners != expected_statements
    {
        return Err(ReplayIndexAdmissionError::InvalidStatementPartition);
    }
    let empty_digest = empty_baseline_digest();
    if decoded.baseline_records.len() != decoded.owner_partition.len() {
        return Err(ReplayIndexAdmissionError::InvalidBaselinePartition);
    }
    let mut baseline_record_ranges = Vec::with_capacity(decoded.owner_partition.len());
    for (index, (owner, record)) in decoded
        .owner_partition
        .iter()
        .zip(&decoded.baseline_records)
        .enumerate()
    {
        if record.owner != *owner
            || (!matches!(record.owner, ReplayOwner::Statement(_))
                && (record.record_count != 0 || record.digest != empty_digest))
        {
            return Err(ReplayIndexAdmissionError::InvalidBaselinePartition);
        }
        let end = index
            .checked_add(1)
            .ok_or(ReplayIndexAdmissionError::InvalidBaselinePartition)?;
        let range = ReplayRowRange::checked(index, end)
            .map_err(|_| ReplayIndexAdmissionError::InvalidBaselinePartition)?;
        baseline_record_ranges.push(range);
    }
    if decoded.unowned_demand_count != 0
        || decoded.invalid_owner_site_count != 0
        || decoded.noncanonical_edge_count != 0
        || decoded.typed_reference_coverage_misses != 0
    {
        return Err(ReplayIndexAdmissionError::NonzeroGenerationHealthCounter);
    }
    if expected_manifest_sha256
        .is_some_and(|expected| decoded.canonical_manifest_sha256 != expected)
    {
        return Err(ReplayIndexAdmissionError::ManifestIdentityMismatch);
    }

    Ok(AdmittedCollisionReplayIndex {
        schema: decoded.schema,
        owner_partition: decoded.owner_partition,
        root_slots: decoded.root_slots,
        owner_sites: decoded.owner_sites,
        reverse_edges: decoded.reverse_edges,
        root_slot_consumers: decoded.root_slot_consumers,
        scc_membership: decoded.scc_membership,
        statement_owners: decoded.statement_owners,
        baseline_records: decoded.baseline_records,
        unowned_demand_count: decoded.unowned_demand_count,
        invalid_owner_site_count: decoded.invalid_owner_site_count,
        noncanonical_edge_count: decoded.noncanonical_edge_count,
        typed_reference_coverage_misses: decoded.typed_reference_coverage_misses,
        owner_to_scc,
        scc_owner_ranges,
        scc_owners,
        reverse_scc_offsets,
        reverse_scc_edges,
        root_slot_lookup,
        owner_site_ranges,
        baseline_record_ranges,
        canonical_manifest_len,
        canonical_manifest_sha256: decoded.canonical_manifest_sha256,
    })
}

fn empty_baseline_digest() -> [u8; 32] {
    let mut bytes = ManifestBytes::new(b"typokat-collision-replay-owner-records-v1");
    bytes.u64(0);
    Sha256::digest(bytes.finish()).into()
}

pub fn baseline_record(
    owner: ReplayOwner,
    records: &[Vec<u8>],
) -> Result<ReplayBaselineRecord, ReplayIndexGenerationError> {
    let mut bytes = ManifestBytes::new(b"typokat-collision-replay-owner-records-v1");
    bytes.u64(
        u64::try_from(records.len()).map_err(|_| ReplayIndexGenerationError::IntegerOverflow)?,
    );
    for record in records {
        bytes.bytes(record)?;
    }
    Ok(ReplayBaselineRecord {
        owner,
        record_count: u64::try_from(records.len())
            .map_err(|_| ReplayIndexGenerationError::IntegerOverflow)?,
        digest: Sha256::digest(bytes.finish()).into(),
    })
}

struct ManifestBytes(Vec<u8>);

impl ManifestBytes {
    fn new(domain: &[u8]) -> Self {
        Self(domain.to_vec())
    }

    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }

    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    fn usize(&mut self, value: usize) -> Result<(), ReplayIndexGenerationError> {
        self.u64(u64::try_from(value).map_err(|_| ReplayIndexGenerationError::IntegerOverflow)?);
        Ok(())
    }

    fn optional_u32(&mut self, value: Option<u32>) {
        self.u8(u8::from(value.is_some()));
        if let Some(value) = value {
            self.u32(value);
        }
    }

    fn bytes(&mut self, value: &[u8]) -> Result<(), ReplayIndexGenerationError> {
        self.usize(value.len())?;
        self.raw(value);
        Ok(())
    }

    fn string(&mut self, value: &str) -> Result<(), ReplayIndexGenerationError> {
        self.bytes(value.as_bytes())
    }

    fn raw(&mut self, value: &[u8]) {
        self.0.extend_from_slice(value);
    }

    fn finish(self) -> Vec<u8> {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(file: usize, start: u32) -> LibraryEventKey {
        LibraryEventKey {
            file_ordinal: LibraryFileOrdinal::new(file),
            source_start: start,
            event_ordinal: 0,
            record_ordinal: 0,
        }
    }

    fn site(owner: ReplayOwner) -> ReplayOwnerSite {
        ReplayOwnerSite {
            owner,
            file_ordinal: LibraryFileOrdinal::new(0),
            span: Span::new(0, 1),
        }
    }

    fn baselines(owners: &[ReplayOwner]) -> Vec<ReplayBaselineRecord> {
        owners
            .iter()
            .copied()
            .map(|owner| baseline_record(owner, &[]).unwrap())
            .collect()
    }

    #[test]
    fn owner_tags_and_order_are_stable() {
        let owners = [
            ReplayOwner::TypeGroup(TypeGroupId(1)),
            ReplayOwner::Value(ValueStorageId(1)),
            ReplayOwner::Namespace(NamespaceId(1)),
            ReplayOwner::Class(ClassId(1)),
            ReplayOwner::GlobalObject,
            ReplayOwner::Statement(key(0, 1)),
        ];
        assert_eq!(owners.map(ReplayOwner::tag), [0, 1, 2, 3, 4, 5]);
        assert!(owners.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn owner_scope_restores_nesting_and_counts_unowned_demand() {
        let trace = ReplayDependencyTrace::default();
        let outer = ReplayOwner::TypeGroup(TypeGroupId(1));
        let inner = ReplayOwner::Value(ValueStorageId(2));
        trace.enter(outer);
        trace.enter(inner);
        trace.demand(outer);
        trace.leave(inner);
        assert_eq!(trace.current_owner(), Some(outer));
        trace.leave(outer);
        trace.demand(inner);
        assert_eq!(trace.counters(), (1, 0));
    }

    #[test]
    fn owner_scope_restores_after_unwind() {
        let trace = ReplayDependencyTrace::default();
        let owner = ReplayOwner::GlobalObject;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe({
            let trace = trace.clone();
            move || {
                let _scope = trace.scope(owner);
                panic!("scope restoration witness");
            }
        }));
        assert!(outcome.is_err());
        assert_eq!(trace.current_owner(), None);
    }

    #[test]
    fn reverse_edges_are_oriented_dependency_to_consumer_and_deduplicated() {
        let trace = ReplayDependencyTrace::default();
        let consumer = ReplayOwner::TypeGroup(TypeGroupId(2));
        let dependency = ReplayOwner::TypeGroup(TypeGroupId(1));
        trace.enter(consumer);
        trace.demand(dependency);
        trace.demand(dependency);
        trace.leave(consumer);
        let owners = vec![consumer, dependency];
        let baseline_records = baselines(&owners);
        let index = trace
            .finish(
                owners.clone(),
                Vec::new(),
                owners.into_iter().map(site).collect(),
                baseline_records,
                0,
            )
            .unwrap();
        assert_eq!(
            index.reverse_edges,
            [ReplayReverseEdge {
                dependency,
                consumer
            }]
        );
    }

    #[test]
    fn dependency_first_scc_order_handles_a_long_chain_without_drift() {
        let trace = ReplayDependencyTrace::default();
        let owners = (0..20_000)
            .map(|id| ReplayOwner::TypeGroup(TypeGroupId(id)))
            .collect::<Vec<_>>();
        for pair in owners.windows(2) {
            trace.enter(pair[1]);
            trace.demand(pair[0]);
            trace.leave(pair[1]);
        }
        let baseline_records = baselines(&owners);
        let index = trace
            .finish(
                owners.clone(),
                Vec::new(),
                owners.iter().copied().map(site).collect(),
                baseline_records,
                0,
            )
            .unwrap();
        assert_eq!(index.scc_membership.len(), owners.len());
        assert_eq!(index.scc_membership[0].owners.as_slice(), [owners[0]]);
        assert_eq!(
            index.scc_membership.last().unwrap().owners.as_slice(),
            [*owners.last().unwrap()]
        );
    }

    #[test]
    fn statement_owners_use_canonical_event_keys() {
        let statement = key(7, 41);
        let owner = ReplayOwner::Statement(statement);
        let mut seed = ReplayTraceSeed::new(OwnerSiteStorageMode::Flat);
        assert!(seed.reserve_owner_site_ticket((0, 0), statement));
        let trace = ReplayDependencyTrace::new(seed);
        let index = trace
            .finish(
                vec![owner],
                Vec::new(),
                vec![site(owner)],
                vec![baseline_record(owner, &[]).unwrap()],
                0,
            )
            .unwrap();
        assert_eq!(index.statement_owners, [(statement, owner)]);
    }

    #[test]
    fn baseline_record_digest_is_domain_separated_and_count_sensitive() {
        let owner = ReplayOwner::GlobalObject;
        let empty = baseline_record(owner, &[]).unwrap();
        let one = baseline_record(owner, &[b"record".to_vec()]).unwrap();
        assert_eq!(empty.digest, empty_baseline_digest());
        assert_ne!(empty.digest, one.digest);
        assert_eq!(one.record_count, 1);
    }

    #[test]
    fn invalid_provenance_fails_generation() {
        let owner = ReplayOwner::GlobalObject;
        let error = ReplayDependencyTrace::default()
            .finish(vec![owner], Vec::new(), Vec::new(), baselines(&[owner]), 0)
            .unwrap_err();
        assert_eq!(error, ReplayIndexGenerationError::MissingOwnerSite(owner));
    }

    #[test]
    fn active_owner_without_a_typed_dependency_fails_coverage() {
        let owner = ReplayOwner::GlobalObject;
        let trace = ReplayDependencyTrace::default();
        trace.enter(owner);
        drop(trace.observe_typed_demand("missing-test-edge"));
        trace.leave(owner);
        let error = trace
            .finish(
                vec![owner],
                Vec::new(),
                vec![site(owner)],
                baselines(&[owner]),
                0,
            )
            .unwrap_err();
        assert_eq!(
            error,
            ReplayIndexGenerationError::TypedReferenceCoverage {
                count: 1,
                samples: vec![ReplayCoverageMiss::Boundary("missing-test-edge")],
            }
        );
    }

    #[test]
    fn canonical_manifest_is_permutation_invariant() {
        let first = ReplayOwner::Value(ValueStorageId(1));
        let second = ReplayOwner::TypeGroup(TypeGroupId(2));
        let build = |reverse: bool| {
            let trace = ReplayDependencyTrace::default();
            trace.enter(second);
            trace.demand(first);
            trace.leave(second);
            let mut owners = vec![first, second];
            let mut sites = owners.iter().copied().map(site).collect::<Vec<_>>();
            let mut baselines = baselines(&owners);
            let mut roots = vec![
                ReplayRootSlot {
                    name: "B".to_owned(),
                    value: Some(ValueStorageId(1)),
                    ty: None,
                    namespace: None,
                    global_object_contributor: false,
                    explicit_global_this: false,
                },
                ReplayRootSlot {
                    name: "A".to_owned(),
                    value: None,
                    ty: Some(TypeGroupId(2)),
                    namespace: None,
                    global_object_contributor: false,
                    explicit_global_this: false,
                },
            ];
            if reverse {
                owners.reverse();
                sites.reverse();
                baselines.reverse();
                roots.reverse();
            }
            trace.finish(owners, roots, sites, baselines, 0).unwrap()
        };
        let forward = build(false);
        let reverse = build(true);
        assert_eq!(
            forward.canonical_manifest_bytes,
            reverse.canonical_manifest_bytes
        );
        assert_eq!(
            forward.canonical_manifest_sha256,
            reverse.canonical_manifest_sha256
        );
    }

    #[test]
    fn malformed_edge_rows_increment_the_real_validator_counter() {
        let owner = ReplayOwner::GlobalObject;
        let partition = BTreeSet::from([owner]);
        assert_eq!(
            validate_edge_rows(
                &partition,
                &[ReplayReverseEdge {
                    dependency: owner,
                    consumer: owner,
                }],
                &[],
            ),
            1
        );
    }

    fn finish_dependency_fixture(
        trace: ReplayDependencyTrace,
        owners: Vec<ReplayOwner>,
    ) -> Result<CollisionReplayIndex, ReplayIndexGenerationError> {
        trace.finish(
            owners.clone(),
            Vec::new(),
            owners.iter().copied().map(site).collect(),
            baselines(&owners),
            0,
        )
    }

    #[test]
    fn typed_coverage_accepts_a_forward_transitive_class_dependency() {
        let consumer = ReplayOwner::TypeGroup(TypeGroupId(1));
        let intermediate = ReplayOwner::Value(ValueStorageId(2));
        let class = ReplayOwner::Class(ClassId(3));
        let trace = ReplayDependencyTrace::default();
        trace.enter(consumer);
        trace.demand(intermediate);
        trace.leave(consumer);
        trace.enter(intermediate);
        trace.demand(class);
        trace.leave(intermediate);
        trace.require_dependency(consumer, class);
        assert!(finish_dependency_fixture(trace, vec![consumer, intermediate, class]).is_ok());
    }

    #[test]
    fn typed_coverage_rejects_a_reverse_only_class_edge() {
        let consumer = ReplayOwner::TypeGroup(TypeGroupId(1));
        let class = ReplayOwner::Class(ClassId(3));
        let trace = ReplayDependencyTrace::default();
        trace.enter(class);
        trace.demand(consumer);
        trace.leave(class);
        trace.require_dependency(consumer, class);
        let error = finish_dependency_fixture(trace, vec![consumer, class]).unwrap_err();
        assert!(matches!(
            error,
            ReplayIndexGenerationError::TypedReferenceCoverage { count: 1, .. }
        ));
    }

    #[test]
    fn typed_coverage_accepts_an_scc_with_an_outgoing_class_dependency() {
        let consumer = ReplayOwner::TypeGroup(TypeGroupId(1));
        let peer = ReplayOwner::Value(ValueStorageId(2));
        let class = ReplayOwner::Class(ClassId(3));
        let trace = ReplayDependencyTrace::default();
        trace.enter(consumer);
        trace.demand(peer);
        trace.leave(consumer);
        trace.enter(peer);
        trace.demand(consumer);
        trace.demand(class);
        trace.leave(peer);
        trace.require_dependency(consumer, class);
        assert!(finish_dependency_fixture(trace, vec![consumer, peer, class]).is_ok());
    }

    #[test]
    fn typed_coverage_does_not_reverse_an_unrelated_dependency_chain() {
        let consumer = ReplayOwner::TypeGroup(TypeGroupId(1));
        let unrelated = ReplayOwner::Value(ValueStorageId(2));
        let class = ReplayOwner::Class(ClassId(3));
        let trace = ReplayDependencyTrace::default();
        trace.enter(class);
        trace.demand(unrelated);
        trace.leave(class);
        trace.enter(unrelated);
        trace.demand(consumer);
        trace.leave(unrelated);
        trace.require_dependency(consumer, class);
        let error = finish_dependency_fixture(trace, vec![consumer, unrelated, class]).unwrap_err();
        assert!(matches!(
            error,
            ReplayIndexGenerationError::TypedReferenceCoverage { count: 1, .. }
        ));
    }
}

#[cfg(any(test, feature = "test-utils"))]
mod raw_access_audit {
    fn repository_root() -> Result<std::path::PathBuf, String> {
        let current = std::env::current_dir()
            .map_err(|error| format!("cannot read test process current directory: {error}"))?;
        current
            .ancestors()
            .find(|candidate| {
                let Ok(manifest) = std::fs::read_to_string(candidate.join("Cargo.toml")) else {
                    return false;
                };
                manifest.starts_with("[package]\nname = \"typokat\"\n")
                    && candidate.join("Cargo.lock").is_file()
                    && candidate.join("src/lib.rs").is_file()
            })
            .map(std::path::Path::to_path_buf)
            .ok_or_else(|| {
                format!(
                    "test process must run within the typokat repository: {}",
                    current.display()
                )
            })
    }

    fn normalize_source(source: &str) -> String {
        let bytes = source.as_bytes();
        let mut normalized = String::with_capacity(source.len());
        let mut cursor = 0usize;
        while cursor < bytes.len() {
            if bytes.get(cursor..cursor + 2) == Some(b"//")
                || bytes.get(cursor..cursor + 2) == Some(b"/*")
            {
                cursor = skip_non_code(bytes, cursor).unwrap_or(bytes.len());
                continue;
            }
            if let Some(end) = raw_string_end(bytes, cursor) {
                normalized.push_str("\"\"");
                cursor = end;
                continue;
            }
            if bytes[cursor] == b'"' {
                normalized.push_str("\"\"");
                cursor = quoted_end(bytes, cursor, b'"');
                continue;
            }
            if bytes[cursor] == b'\'' {
                let end = quoted_end(bytes, cursor, b'\'');
                if end.saturating_sub(cursor) <= 4 {
                    normalized.push_str("''");
                    cursor = end;
                    continue;
                }
            }
            let Some(character) = source[cursor..].chars().next() else {
                break;
            };
            cursor += character.len_utf8();
            if !character.is_whitespace() {
                normalized.push(character);
            }
        }
        normalized
    }

    const DENIED_RAW_REPLAY_TOKENS: &[&str] = &[
        ".resolve_value_binding",
        ".resolve_value",
        ".resolve_type",
        ".resolve_qualified_type_path",
        ".published_class",
        ".require_class",
        ".require",
        "type_decl_id",
        "value_decl_id",
        "decl_types.get",
        "decl_types.set",
        "class_parents.get",
        "class_parents.iter",
        "class_parents.local_iter",
        "::resolve_value_binding",
        "::resolve_value",
        "::resolve_type",
        "::resolve_qualified_type_path",
        "::published_class",
        "::require_class",
        "DeclTypes::get",
        "DeclTypes::set",
        "PublishedClasses::published_class",
        "PublishedClasses::require",
        "LayeredMap::get",
        "LayeredMap::iter",
        "LayeredMap::local_iter",
    ];

    fn identifier_character(character: char) -> bool {
        character == '_' || character.is_alphanumeric() || !character.is_ascii()
    }

    fn contains_denied_token(source: &str, token: &str) -> bool {
        source.match_indices(token).any(|(start, matched)| {
            let prefix_is_clear = !token.chars().next().is_some_and(identifier_character)
                || source[..start]
                    .chars()
                    .next_back()
                    .is_none_or(|character| !identifier_character(character));
            let end = start + matched.len();
            let suffix_is_clear = !token.chars().next_back().is_some_and(identifier_character)
                || source[end..]
                    .chars()
                    .next()
                    .is_none_or(|character| !identifier_character(character));
            prefix_is_clear && suffix_is_clear
        })
    }

    pub(super) struct RawAccessAllowance {
        path: &'static str,
        snippet: &'static str,
        count: usize,
        reason: &'static str,
    }

    const TEST_ONLY_REPLAY_SOURCE_PATHS: &[&str] = &[
        "crates/typokat-check/src/check/checker/calls/contextual_duplicate_diagnostics_spec.rs",
        "crates/typokat-check/src/check/checker/calls/contextual_rewalk_scaling_spec.rs",
        "crates/typokat-check/src/check/checker/declaration_owner_scaling_spec.rs",
        "crates/typokat-check/src/check/checker/declaration_surface_lazy_spec.rs",
        "crates/typokat-check/src/check/checker/declaration_surface_measure.rs",
        "crates/typokat-check/src/check/checker/exact_declaration_site_cutover_spec.rs",
        "crates/typokat-check/src/check/checker/decls/cycle_tainted_application_cache_spec.rs",
        "crates/typokat-check/src/check/checker/decls/eager_application_cache_spec.rs",
        "crates/typokat-check/src/check/checker/decls/heritage_base_merge_scan_spec.rs",
        "crates/typokat-check/src/check/checker/decls/interface_scc_pending_spec.rs",
        "crates/typokat-check/src/check/checker/eval/deferred_keyof_cache_spec.rs",
        "crates/typokat-check/src/check/checker/eval/tests.rs",
        "crates/typokat-check/src/check/checker/lexical_events/completion_slot_spec.rs",
        "crates/typokat-check/src/check/checker/lexical_events/owner_lookup_spec.rs",
        "crates/typokat-check/src/check/checker/surface_lowering_copy_spec.rs",
        "crates/typokat-check/src/check/query/deferred_indexed_lazy_spec.rs",
        "crates/typokat-check/src/check/query/demand_identity_spec.rs",
        "crates/typokat-check/src/check/query/dom_source_cold_spec.rs",
        "crates/typokat-check/src/check/query/event_listener_union_scaling_spec.rs",
        "crates/typokat-check/src/check/query/failing_relation_scaling_spec.rs",
        "crates/typokat-check/src/check/query/identity_memo_spec.rs",
        "crates/typokat-check/src/check/query/instantiation_root_lazy_spec.rs",
        "crates/typokat-check/src/check/query/relation_root_lazy_spec.rs",
        "crates/typokat-check/src/check/query/tests.rs",
        "crates/typokat-check/src/check/query/transaction_fork_scaling_spec.rs",
    ];

    const EXPECTED_PRODUCTION_REPLAY_SOURCE_PATHS: &[&str] = &[
        "crates/typokat-check/src/check/checker/annotations/composites.rs",
        "crates/typokat-check/src/check/checker/annotations/declared.rs",
        "crates/typokat-check/src/check/checker/annotations/functions.rs",
        "crates/typokat-check/src/check/checker/annotations/mod.rs",
        "crates/typokat-check/src/check/checker/annotations/signatures.rs",
        "crates/typokat-check/src/check/checker/annotations/type_operators.rs",
        "crates/typokat-check/src/check/checker/assignment.rs",
        "crates/typokat-check/src/check/checker/calls.rs",
        "crates/typokat-check/src/check/checker/classes/application.rs",
        "crates/typokat-check/src/check/checker/classes/body.rs",
        "crates/typokat-check/src/check/checker/classes/construction.rs",
        "crates/typokat-check/src/check/checker/classes/inheritance.rs",
        "crates/typokat-check/src/check/checker/classes/initializer.rs",
        "crates/typokat-check/src/check/checker/classes/members.rs",
        "crates/typokat-check/src/check/checker/classes/mod.rs",
        "crates/typokat-check/src/check/checker/classes/publication.rs",
        "crates/typokat-check/src/check/checker/classes/retained.rs",
        "crates/typokat-check/src/check/checker/classes/surface_types.rs",
        "crates/typokat-check/src/check/checker/classes/type_syntax.rs",
        "crates/typokat-check/src/check/checker/classes/visibility.rs",
        "crates/typokat-check/src/check/checker/context.rs",
        "crates/typokat-check/src/check/checker/decls/interface.rs",
        "crates/typokat-check/src/check/checker/decls/mod.rs",
        "crates/typokat-check/src/check/checker/decls/params.rs",
        "crates/typokat-check/src/check/checker/decls/resolve.rs",
        "crates/typokat-check/src/check/checker/eval/demand.rs",
        "crates/typokat-check/src/check/checker/eval/extends.rs",
        "crates/typokat-check/src/check/checker/eval/instantiation.rs",
        "crates/typokat-check/src/check/checker/eval/keyof.rs",
        "crates/typokat-check/src/check/checker/eval/mapped.rs",
        "crates/typokat-check/src/check/checker/eval/mod.rs",
        "crates/typokat-check/src/check/checker/eval/template.rs",
        "crates/typokat-check/src/check/checker/events.rs",
        "crates/typokat-check/src/check/checker/events_library.rs",
        "crates/typokat-check/src/check/checker/expr.rs",
        "crates/typokat-check/src/check/checker/flowgraph/exprs.rs",
        "crates/typokat-check/src/check/checker/flowgraph/mod.rs",
        "crates/typokat-check/src/check/checker/flowgraph/nodes.rs",
        "crates/typokat-check/src/check/checker/function_groups.rs",
        "crates/typokat-check/src/check/checker/indexed_access.rs",
        "crates/typokat-check/src/check/checker/lexical_events.rs",
        "crates/typokat-check/src/check/checker/lexical_events_library.rs",
        "crates/typokat-check/src/check/checker/lexical_events_user.rs",
        "crates/typokat-check/src/check/checker/library_compiler.rs",
        "crates/typokat-check/src/check/checker/library_identities.rs",
        "crates/typokat-check/src/check/checker/library_reporting.rs",
        "crates/typokat-check/src/check/checker/mod.rs",
        "crates/typokat-check/src/check/checker/namespace_values.rs",
        "crates/typokat-check/src/check/checker/narrowing.rs",
        "crates/typokat-check/src/check/checker/replay_index.rs",
        "crates/typokat-check/src/check/checker/reporting_record.rs",
        "crates/typokat-check/src/check/checker/statements.rs",
        "crates/typokat-check/src/check/checker/type_groups.rs",
        "crates/typokat-check/src/check/query/mod.rs",
    ];

    fn raw_string_end(bytes: &[u8], start: usize) -> Option<usize> {
        let mut cursor = start;
        if bytes.get(cursor) == Some(&b'b') {
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&b'r') {
            return None;
        }
        cursor += 1;
        let hashes = bytes[cursor..]
            .iter()
            .take_while(|byte| **byte == b'#')
            .count();
        cursor += hashes;
        if bytes.get(cursor) != Some(&b'"') {
            return None;
        }
        cursor += 1;
        while cursor < bytes.len() {
            if bytes[cursor] == b'"'
                && bytes
                    .get(cursor + 1..cursor + 1 + hashes)
                    .is_some_and(|suffix| suffix.iter().all(|byte| *byte == b'#'))
            {
                return Some(cursor + 1 + hashes);
            }
            cursor += 1;
        }
        Some(bytes.len())
    }

    fn quoted_end(bytes: &[u8], start: usize, quote: u8) -> usize {
        let mut cursor = start + 1;
        while cursor < bytes.len() {
            match bytes[cursor] {
                b'\\' => cursor = (cursor + 2).min(bytes.len()),
                byte if byte == quote => return cursor + 1,
                _ => cursor += 1,
            }
        }
        bytes.len()
    }

    fn skip_non_code(bytes: &[u8], cursor: usize) -> Option<usize> {
        if bytes.get(cursor..cursor + 2) == Some(b"//") {
            return Some(
                bytes[cursor + 2..]
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .map_or(bytes.len(), |offset| cursor + 3 + offset),
            );
        }
        if bytes.get(cursor..cursor + 2) == Some(b"/*") {
            let mut depth = 1usize;
            let mut end = cursor + 2;
            while end < bytes.len() && depth != 0 {
                if bytes.get(end..end + 2) == Some(b"/*") {
                    depth += 1;
                    end += 2;
                } else if bytes.get(end..end + 2) == Some(b"*/") {
                    depth -= 1;
                    end += 2;
                } else {
                    end += 1;
                }
            }
            return Some(end);
        }
        if let Some(end) = raw_string_end(bytes, cursor) {
            return Some(end);
        }
        if bytes[cursor] == b'"' {
            return Some(quoted_end(bytes, cursor, b'"'));
        }
        if bytes[cursor] == b'\'' {
            let end = quoted_end(bytes, cursor, b'\'');
            if end.saturating_sub(cursor) <= 4 {
                return Some(end);
            }
        }
        None
    }

    fn cfg_test_item_end(source: &str, item_start: usize) -> usize {
        let bytes = source.as_bytes();
        let mut cursor = item_start;
        let mut parentheses = 0usize;
        let mut brackets = 0usize;
        let mut braces = 0usize;
        while cursor < bytes.len() {
            if let Some(end) = skip_non_code(bytes, cursor) {
                cursor = end;
                continue;
            }
            match bytes[cursor] {
                b'(' => parentheses += 1,
                b')' => parentheses = parentheses.saturating_sub(1),
                b'[' => brackets += 1,
                b']' => brackets = brackets.saturating_sub(1),
                b'{' => braces += 1,
                b'}' if braces != 0 => {
                    braces -= 1;
                    if braces == 0 && parentheses == 0 && brackets == 0 {
                        cursor += 1;
                        while cursor < bytes.len() {
                            if let Some(end) = skip_non_code(bytes, cursor) {
                                cursor = end;
                            } else if bytes[cursor].is_ascii_whitespace() {
                                cursor += 1;
                            } else {
                                break;
                            }
                        }
                        if matches!(bytes.get(cursor), Some(b',' | b';')) {
                            cursor += 1;
                        }
                        return cursor;
                    }
                }
                b';' if parentheses == 0 && brackets == 0 && braces == 0 => return cursor + 1,
                _ => {}
            }
            cursor += 1;
        }
        bytes.len()
    }

    fn strip_keyword<'a>(source: &'a str, keyword: &str) -> Option<&'a str> {
        let remainder = source.strip_prefix(keyword)?;
        if remainder
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            return None;
        }
        Some(remainder)
    }

    fn cfg_test_starts_item(source: &str, item_start: usize) -> bool {
        let mut remainder = source[item_start..].trim_start();
        while remainder.starts_with("#[") {
            let bytes = remainder.as_bytes();
            let mut cursor = 2usize;
            let mut brackets = 1usize;
            while cursor < bytes.len() && brackets != 0 {
                if let Some(end) = skip_non_code(bytes, cursor) {
                    cursor = end;
                    continue;
                }
                match bytes[cursor] {
                    b'[' => brackets += 1,
                    b']' => brackets -= 1,
                    _ => {}
                }
                cursor += 1;
            }
            remainder = remainder[cursor..].trim_start();
        }

        if let Some(after_pub) = strip_keyword(remainder, "pub") {
            remainder = after_pub.trim_start();
            if remainder.starts_with('(') {
                let Some(end) = remainder.find(')') else {
                    return false;
                };
                remainder = remainder[end + 1..].trim_start();
            }
        }
        while let Some(after_wrapper) = ["async", "unsafe", "default"]
            .iter()
            .find_map(|wrapper| strip_keyword(remainder, wrapper))
        {
            remainder = after_wrapper.trim_start();
        }
        if let Some(after_extern) = strip_keyword(remainder, "extern") {
            remainder = after_extern.trim_start();
            if remainder.starts_with('"') {
                remainder = &remainder[quoted_end(remainder.as_bytes(), 0, b'"')..];
            }
            remainder = remainder.trim_start();
        }

        [
            "fn",
            "mod",
            "impl",
            "use",
            "const",
            "static",
            "struct",
            "enum",
            "type",
            "trait",
            "union",
            "macro",
            "macro_rules",
        ]
        .iter()
        .any(|keyword| strip_keyword(remainder, keyword).is_some())
    }

    fn next_test_only_attribute(source: &str, search_from: usize) -> Option<(usize, usize)> {
        const TEST_ONLY_ATTRIBUTES: [&[u8]; 2] = [
            b"#[cfg(test)]",
            b"#[cfg(any(test, feature = \"test-utils\"))]",
        ];
        let bytes = source.as_bytes();
        let mut cursor = search_from;
        while cursor < bytes.len() {
            if let Some(end) = skip_non_code(bytes, cursor) {
                cursor = end.max(cursor + 1);
                continue;
            }
            if let Some(attribute) = TEST_ONLY_ATTRIBUTES
                .iter()
                .find(|attribute| bytes.get(cursor..cursor + attribute.len()) == Some(**attribute))
            {
                return Some((cursor, attribute.len()));
            }
            cursor += 1;
        }
        None
    }

    pub(super) fn production_source(source: &str) -> String {
        let mut stripped = source.to_owned();
        let mut search_from = 0usize;
        while let Some((start, attribute_len)) = next_test_only_attribute(&stripped, search_from) {
            let item_start = start + attribute_len;
            let end = if cfg_test_starts_item(&stripped, item_start) {
                cfg_test_item_end(&stripped, item_start)
            } else {
                item_start
            };
            stripped.replace_range(start..end, " ");
            search_from = start + 1;
        }
        stripped
    }

    pub(super) fn discover_production_rust_sources(
        root: &std::path::Path,
    ) -> std::collections::BTreeMap<String, String> {
        fn visit(
            root: &std::path::Path,
            directory: &std::path::Path,
            sources: &mut std::collections::BTreeMap<String, String>,
        ) {
            let mut entries = std::fs::read_dir(directory)
                .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
                .collect::<Result<Vec<_>, _>>()
                .expect("read checker source entries");
            entries.sort_by_key(std::fs::DirEntry::file_name);
            for entry in entries {
                let path = entry.path();
                if path.is_dir() {
                    if entry.file_name() != "tests" {
                        visit(root, &path, sources);
                    }
                    continue;
                }
                if path.extension().and_then(std::ffi::OsStr::to_str) != Some("rs") {
                    continue;
                }
                let relative = path
                    .strip_prefix(root)
                    .expect("source stays below manifest root")
                    .to_string_lossy()
                    .replace('\\', "/");
                if TEST_ONLY_REPLAY_SOURCE_PATHS.contains(&relative.as_str()) {
                    continue;
                }
                let source = std::fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
                assert!(sources.insert(relative, source).is_none());
            }
        }

        let mut sources = std::collections::BTreeMap::new();
        visit(
            root,
            &root.join("crates/typokat-check/src/check/checker"),
            &mut sources,
        );
        visit(
            root,
            &root.join("crates/typokat-check/src/check/query"),
            &mut sources,
        );
        sources
    }

    pub(super) fn validate_production_source_set(
        sources: &std::collections::BTreeMap<String, String>,
        expected_paths: &[&str],
    ) -> Result<(), String> {
        let actual = sources
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        let expected = expected_paths
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        if actual == expected {
            return Ok(());
        }
        let missing = expected.difference(&actual).copied().collect::<Vec<_>>();
        let unexpected = actual.difference(&expected).copied().collect::<Vec<_>>();
        Err(format!(
            "production replay source manifest drift: missing={missing:?}, unexpected={unexpected:?}"
        ))
    }

    pub(super) fn validate_raw_access_manifest(
        sources: &std::collections::BTreeMap<String, String>,
        expected_paths: &[&str],
        allowed: &[RawAccessAllowance],
    ) -> Result<(), String> {
        validate_production_source_set(sources, expected_paths)?;
        let mut production = sources
            .iter()
            .map(|(path, source)| (path.clone(), normalize_source(&production_source(source))))
            .collect::<std::collections::BTreeMap<_, _>>();
        for allowance in allowed {
            let Some(remainder) = production.get_mut(allowance.path) else {
                return Err(format!(
                    "raw replay allowance references undiscovered file {}: {}",
                    allowance.path, allowance.reason
                ));
            };
            let snippet = normalize_source(allowance.snippet);
            let actual = remainder.matches(&snippet).count();
            if actual != allowance.count {
                return Err(format!(
                    "raw replay allowance drift in {}: {} (expected {}, found {})",
                    allowance.path, allowance.reason, allowance.count, actual
                ));
            }
            *remainder = remainder.replacen(&snippet, "", allowance.count);
        }
        for (path, remainder) in &production {
            for token in DENIED_RAW_REPLAY_TOKENS {
                if contains_denied_token(remainder, token) {
                    return Err(format!("unapproved raw replay access `{token}` in {path}"));
                }
            }
        }
        Ok(())
    }

    pub(super) fn audit() -> Result<super::RawSemanticAccessAuditForTest, String> {
        let root = repository_root()?;
        let sources = discover_production_rust_sources(&root);
        let allowed = [
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/mod.rs", snippet: "self.class_parents.iter()", count: 1, reason: "snapshot projection" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/mod.rs", snippet: "use decls::{\n    reserve_type_decls, reserve_type_decls_for_combined_library, reserve_type_decls_selected,\n    type_decl_id, walk_type_decls, TopTypeDecl,\n};", count: 1, reason: "prelude identity and selected private replay helper imports" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/mod.rs", snippet: "type_decl_id(binder, binder.prelude_module, name)", count: 1, reason: "prelude identity selection" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/mod.rs", snippet: "return self.binder.resolve_value_binding(scope, name);", count: 1, reason: "no-trace value binding fast path" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/mod.rs", snippet: "return self.binder.resolve_value(scope, name);", count: 1, reason: "no-trace value fast path" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/mod.rs", snippet: "return self.binder.resolve_type(scope, name);", count: 1, reason: "no-trace type fast path" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/mod.rs", snippet: "return self.binder.resolve_qualified_type_path(scope, segments);", count: 1, reason: "no-trace qualified fast path" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/mod.rs", snippet: ".classes().published_class(class)", count: 1, reason: "instrumented class delegate" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/mod.rs", snippet: "return self.decl_types.get(storage);", count: 1, reason: "no-trace decl fast path" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/mod.rs", snippet: "self.decl_types.get(storage)", count: 1, reason: "instrumented decl delegate" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/mod.rs", snippet: "decl_types.set(decl_id, error)", count: 2, reason: "module placeholder bootstrap, once per project-check path (prelude and default-library base)" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/mod.rs", snippet: "|pass| pass.decl_types.set(storage, ty)", count: 1, reason: "owned copied publication" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/assignment.rs", snippet: "binder.resolve_value(scope, name)", count: 1, reason: "current binding identity" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/calls.rs", snippet: ".binder.resolve_value(scope, &name)", count: 5, reason: "current callable parameter identity" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/calls.rs", snippet: ".binder.resolve_value(scope, &n)", count: 1, reason: "current callable parameter identity" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/classes/application.rs", snippet: "published.require_class(class)", count: 1, reason: "instrumented class-demand boundary" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/statements.rs", snippet: "self.decl_types.get(decl_id)", count: 2, reason: "own-target cache invalidation" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/statements.rs", snippet: "self.binder.resolve_value(scope, id.name.as_str())", count: 1, reason: "current declaration symbol" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/statements.rs", snippet: ".binder.resolve_value(scope, name)", count: 2, reason: "current function group symbol" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/statements.rs", snippet: "binder.resolve_value(declaration_scope, identifier.name.as_str())", count: 1, reason: "declaration owner lookup" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/classes/publication.rs", snippet: "use crate::check::checker::decls::type_decl_id;", count: 1, reason: "surface resolver helper import" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/classes/publication.rs", snippet: "type_decl_id(self.binder, self.scope, name)", count: 1, reason: "no-trace surface resolver" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/classes/publication.rs", snippet: "self.binder.resolve_qualified_type_path(self.scope, segments)", count: 1, reason: "no-trace qualified resolver" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/classes/publication.rs", snippet: ".staged_published_classes.as_ref().expect(\"class publication is staged\").published_class(class)", count: 1, reason: "instrumented staged class delegate" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/classes/publication.rs", snippet: ".binder.resolve_value(scope, identifier.name.as_str())", count: 1, reason: "class initializer value cache stages exact identity before traced demand" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/classes/publication.rs", snippet: "self.decl_types\n                            .get(storage)", count: 1, reason: "class initializer value cache stages the terminal before traced demand" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/classes/publication.rs", snippet: "self.decl_types.set(value_decl, static_side)", count: 1, reason: "class-owned construction staging" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/classes/publication.rs", snippet: "self.decl_types.set(value_decl, surface.static_template())", count: 1, reason: "value-owned final class publication" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/classes/publication.rs", snippet: "self.class_parents.get(&class_id)", count: 1, reason: "child-owned parent metadata" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/classes/inheritance.rs", snippet: "self.class_parents.get(&current)", count: 1, reason: "child-owned parent chain" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/decls/interface.rs", snippet: ".expect(\"class registry is frozen before interface heritage construction\").published_class(application.class)", count: 1, reason: "instrumented interface class projection" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/namespace_values.rs", snippet: ".expect(\"class publication precedes namespace finalization\").published_class(*class)", count: 1, reason: "instrumented staged class delegate" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/namespace_values.rs", snippet: ".binder.resolve_value(self.scope, identifier.name.as_str())", count: 1, reason: "root self-storage census" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/decls/mod.rs", snippet: "type_decl_id(binder, scope, \"Array\")", count: 1, reason: "topology prepass" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/decls/mod.rs", snippet: "type_decl_id(binder, scope, name)", count: 1, reason: "topology prepass" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/decls/mod.rs", snippet: "binder.resolve_type(scope, name)", count: 2, reason: "topology ambiguity check and helper sink" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/decls/mod.rs", snippet: "binder.resolve_value(scope, name)", count: 1, reason: "topology ambiguity check" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/decls/mod.rs", snippet: "binder.resolve_qualified_type_path(scope, segments)", count: 1, reason: "topology prepass" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/decls/mod.rs", snippet: "fn type_decl_id(", count: 1, reason: "raw helper definition" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/library_compiler.rs", snippet: "parts.runtime.class_parents.iter()", count: 2, reason: "snapshot structural validation" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/replay_index.rs", snippet: "self.published.published_class(class)", count: 1, reason: "instrumented replay class delegate" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/replay_index.rs", snippet: "self.published.require(class)", count: 1, reason: "instrumented replay class requirement" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/checker/type_groups.rs", snippet: "PublishedClassLookup::published_class(&class_lookup, class)", count: 1, reason: "instrumented inherited-class replay delegate" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/query/mod.rs", snippet: "self.published.published_class(source.class)", count: 1, reason: "relation query class projection" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/query/mod.rs", snippet: "self.published.published_class(instance.class)", count: 1, reason: "query application class projection" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/query/mod.rs", snippet: "published.published_class(instance.class)", count: 1, reason: "query publication frontier" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/query/mod.rs", snippet: "PublishedClasses::published_class(self, class)", count: 1, reason: "published-class trait adapter" },
            RawAccessAllowance { path: "crates/typokat-check/src/check/query/mod.rs", snippet: "self.require(class)", count: 1, reason: "published-class trait adapter" },
        ];
        validate_raw_access_manifest(&sources, EXPECTED_PRODUCTION_REPLAY_SOURCE_PATHS, &allowed)?;

        let injection_path = "crates/typokat-check/src/check/checker/annotations/composites.rs";
        let injections = [
            "fn bypass() { binder.resolve_type(scope, name); }",
            "fn bypass() { PublishedClassLookup::published_class(published, class); }",
            "fn bypass() { Binder::resolve_value_binding(binder, scope, name); }",
            "fn bypass() { Binder::resolve_value(binder, scope, name); }",
            "fn bypass() { Binder::resolve_type(binder, scope, name); }",
            "fn bypass() { Binder::resolve_qualified_type_path(binder, scope, segments); }",
            "fn bypass() { DeclTypes::get(decl_types, storage); }",
            "fn bypass() { DeclTypes::set(decl_types, storage, ty); }",
            "fn bypass() { PublishedClasses::published_class(published, class); }",
            "fn bypass() { PublishedClasses::require(published, class); }",
            "fn bypass() { LayeredMap::get(class_parents, &class); }",
            "fn bypass() { LayeredMap::iter(class_parents); }",
            "fn bypass() { LayeredMap::local_iter(class_parents); }",
            "fn bypass() { let raw = Binder::resolve_type; raw(binder, scope, name); }",
            "fn bypass() { Binder::resolve_type /* split */ (binder, scope, name); }",
            "fn bypass() { let raw = DeclTypes::get; raw(decl_types, storage); }",
            "fn bypass() { DeclTypes::set /* split */ (decl_types, storage, ty); }",
            "fn bypass() { let raw = PublishedClasses::published_class; raw(published, class); }",
            "fn bypass() { PublishedClasses::require /* split */ (published, class); }",
            "fn bypass() { let raw = LayeredMap::get; raw(class_parents, &class); }",
            "fn bypass() { LayeredMap::iter /* split */ (class_parents); }",
        ];
        let injected_bypass_rejected = injections.iter().all(|injection| {
            let mut injected = sources.clone();
            let Some(source) = injected.get_mut(injection_path) else {
                return false;
            };
            source.push('\n');
            source.push_str(injection);
            validate_raw_access_manifest(
                &injected,
                EXPECTED_PRODUCTION_REPLAY_SOURCE_PATHS,
                &allowed,
            )
            .is_err()
        });
        let mut non_code_injected = sources.clone();
        let Some(source) = non_code_injected.get_mut(injection_path) else {
            return Err(
                "raw replay injection target is absent from the source manifest".to_owned(),
            );
        };
        source.push_str(
            r###"
                // Binder::resolve_type and decl_types.get are not code.
                /* PublishedClasses::published_class and LayeredMap::iter are not code. */
                const RAW_GUARD_LITERAL: &str =
                    "Binder::resolve_type DeclTypes::set PublishedClasses::require LayeredMap::get";
                const RAW_GUARD_RAW_LITERAL: &str =
                    r#"binder.resolve_type decl_types.get class_parents.iter"#;
            "###,
        );
        let non_code_false_positives_accepted = validate_raw_access_manifest(
            &non_code_injected,
            EXPECTED_PRODUCTION_REPLAY_SOURCE_PATHS,
            &allowed,
        )
        .is_ok();
        Ok(super::RawSemanticAccessAuditForTest {
            source_manifest_complete: true,
            injected_bypass_rejected: injected_bypass_rejected && non_code_false_positives_accepted,
        })
    }
}

#[cfg(test)]
mod raw_access_audit_tests {
    use super::raw_access_audit::{
        discover_production_rust_sources, production_source, validate_production_source_set,
        validate_raw_access_manifest,
    };

    #[test]
    fn semantic_replay_accesses_have_an_exact_source_allowlist() {
        let audit = super::raw_semantic_access_audit_for_test().unwrap();
        assert!(audit.source_manifest_complete);
    }

    #[test]
    fn semantic_replay_access_guard_rejects_an_injected_bypass() {
        let audit = super::raw_semantic_access_audit_for_test().unwrap();
        assert!(audit.injected_bypass_rejected);
    }

    #[test]
    fn semantic_replay_access_guard_preserves_code_after_a_cfg_test_field() {
        let sources = std::collections::BTreeMap::from([(
            "crates/typokat-check/src/check/checker/injected.rs".to_owned(),
            "
                struct Probe {
                    #[cfg(test)]
                    test_only: (u8, [u8; 2]),
                    production: u8,
                }
                fn bypass() { binder.resolve_type(scope, name); }
            "
            .to_owned(),
        )]);
        let error = validate_raw_access_manifest(
            &sources,
            &["crates/typokat-check/src/check/checker/injected.rs"],
            &[],
        )
        .unwrap_err();
        assert!(error.contains("unapproved raw replay access"));
    }

    #[test]
    fn semantic_replay_access_guard_ignores_cfg_markers_in_non_code() {
        let fixtures = [
            "// #[cfg(test)]\nfn bypass() { binder.resolve_type(scope, name); }",
            "const MARKER: &str = \"#[cfg(test)]\";\nfn bypass() { binder.resolve_type(scope, name); }",
            "const MARKER: &str = r###\"#[cfg(test)]\"###;\nfn bypass() { binder.resolve_type(scope, name); }",
        ];
        for source in fixtures {
            let sources = std::collections::BTreeMap::from([(
                "crates/typokat-check/src/check/checker/injected.rs".to_owned(),
                source.to_owned(),
            )]);
            let error = validate_raw_access_manifest(
                &sources,
                &["crates/typokat-check/src/check/checker/injected.rs"],
                &[],
            )
            .unwrap_err();
            assert!(error.contains("unapproved raw replay access"));
        }
    }

    #[test]
    fn cfg_test_stripping_is_conservative_for_variants_and_initializers() {
        let source = "
            enum Probe {
                #[cfg(test)]
                TestOnly(u8, [u8; 2]),
                Production,
            }
            const PROBE: Config = Config {
                #[cfg(test)]
                test_only: make(1 < 2, (1, [2, 3]), Config { value: 4 }),
                production: 5,
            };
            fn bypass() { binder.resolve_type(scope, name); }
        ";
        let production = production_source(source);
        assert!(production.contains("TestOnly"));
        assert!(production.contains("test_only"));
        assert!(production.contains("Production"));
        assert!(production.contains("production: 5"));
        assert!(production.contains("binder.resolve_type(scope, name)"));
    }

    #[test]
    fn cfg_test_stripping_removes_a_generic_item_without_hiding_following_code() {
        let source = "
            #[cfg(test)]
            fn helper<'a, T, U>() {
                binder.resolve_type(scope, name);
            }
            fn bypass() { binder.resolve_value(scope, name); }
        ";
        let production = production_source(source);
        assert!(!production.contains("helper"));
        assert!(!production.contains("binder.resolve_type(scope, name)"));
        assert!(production.contains("binder.resolve_value(scope, name)"));
    }

    #[test]
    fn cfg_test_stripping_removes_test_utils_items() {
        let source = "
            #[cfg(any(test, feature = \"test-utils\"))]
            fn helper() {
                binder.resolve_type(scope, name);
            }
            fn production() {}
        ";
        let production = production_source(source);
        assert!(!production.contains("helper"));
        assert!(!production.contains("binder.resolve_type(scope, name)"));
        assert!(production.contains("fn production()"));
    }

    #[test]
    fn production_source_discovery_recurses_into_query_submodules() {
        let unique = format!(
            "typokat-replay-source-discovery-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock is after Unix epoch")
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        std::fs::create_dir_all(root.join("crates/typokat-check/src/check/checker"))
            .expect("create checker fixture");
        std::fs::create_dir_all(root.join("crates/typokat-check/src/check/query/nested"))
            .expect("create query fixture");
        std::fs::write(
            root.join("crates/typokat-check/src/check/checker/known.rs"),
            "fn known() {}",
        )
        .expect("write checker fixture");
        std::fs::write(
            root.join("crates/typokat-check/src/check/query/mod.rs"),
            "mod nested;",
        )
        .expect("write query root fixture");
        std::fs::write(
            root.join("crates/typokat-check/src/check/query/nested/new.rs"),
            "fn discovered() {}",
        )
        .expect("write nested query fixture");
        std::fs::write(
            root.join("crates/typokat-check/src/check/query/tests.rs"),
            "fn test_only() {}",
        )
        .expect("write excluded query fixture");

        let sources = discover_production_rust_sources(&root);
        assert!(sources.contains_key("crates/typokat-check/src/check/query/nested/new.rs"));
        assert!(!sources.contains_key("crates/typokat-check/src/check/query/tests.rs"));
        let error = validate_production_source_set(
            &sources,
            &[
                "crates/typokat-check/src/check/checker/known.rs",
                "crates/typokat-check/src/check/query/mod.rs",
            ],
        )
        .unwrap_err();
        assert!(
            error.contains("unexpected=[\"crates/typokat-check/src/check/query/nested/new.rs\"]")
        );
    }

    #[test]
    fn semantic_replay_access_guard_rejects_an_unmanifested_production_file() {
        let sources = std::collections::BTreeMap::from([
            (
                "crates/typokat-check/src/check/checker/known.rs".to_owned(),
                "fn safe() {}".to_owned(),
            ),
            (
                "crates/typokat-check/src/check/checker/new_module.rs".to_owned(),
                "fn also_safe() {}".to_owned(),
            ),
        ]);
        let error = validate_raw_access_manifest(
            &sources,
            &["crates/typokat-check/src/check/checker/known.rs"],
            &[],
        )
        .unwrap_err();
        assert!(
            error.contains("unexpected=[\"crates/typokat-check/src/check/checker/new_module.rs\"]")
        );
    }

    #[test]
    fn semantic_replay_access_guard_rejects_a_missing_production_file() {
        let sources = std::collections::BTreeMap::from([(
            "crates/typokat-check/src/check/checker/known.rs".to_owned(),
            "fn safe() {}".to_owned(),
        )]);
        let error = validate_raw_access_manifest(
            &sources,
            &[
                "crates/typokat-check/src/check/checker/known.rs",
                "crates/typokat-check/src/check/checker/missing.rs",
            ],
            &[],
        )
        .unwrap_err();
        assert!(error.contains("missing=[\"crates/typokat-check/src/check/checker/missing.rs\"]"));
    }
}

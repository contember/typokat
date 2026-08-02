//! Source-only routing preflight for frozen-library user checks.

use crate::binder::declaration::SourceBindingSlot;
use crate::binder::declaration::{source_global_binding_census, SourceGlobalBindingCandidate};
use crate::binder::namespace::{source_file_kind, ModuleBindingContext};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use std::collections::{BTreeMap, BTreeSet};

use crate::check::checker::library_compiler::CollisionFreeUserDeltaCapability;

/// Certify a fork used only by focused immutable-prefix tests.
#[cfg(test)]
pub(super) fn issue_caller_certified_capability() -> CollisionFreeUserDeltaCapability {
    preflight_project(&BTreeSet::new(), &[], false).take_capability()
}

use super::base::{
    PrivateCollisionCandidate, PrivateCollisionModuleClassification,
    PrivateCollisionModuleClassificationEntry, PrivateCollisionRouteReceipt, PrivateCollisionSlot,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CollisionRoute {
    SharedDelta,
    PrivateCombined,
    RejectedBeforeSemantics,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct CollisionPreflightWork {
    parse_units: u64,
    source_nodes_visited: u64,
    binding_leaves_visited: u64,
    frozen_name_probes: u64,
}

struct CollisionPreflightOutcome {
    route: CollisionRoute,
    capability: Option<CollisionFreeUserDeltaCapability>,
    module_classifications: Vec<(String, PrivateCollisionModuleClassification)>,
    candidates: BTreeMap<String, SourceGlobalBindingCandidate>,
    reasons: BTreeSet<String>,
    relative_import_edges: usize,
    #[cfg(test)]
    work: CollisionPreflightWork,
}

impl CollisionPreflightOutcome {
    fn take_route(
        mut self,
    ) -> (
        CollisionRoute,
        Option<CollisionFreeUserDeltaCapability>,
        PrivateCollisionRouteReceipt,
    ) {
        let receipt = PrivateCollisionRouteReceipt {
            module_classifications: self
                .module_classifications
                .into_iter()
                .map(
                    |(path, classification)| PrivateCollisionModuleClassificationEntry {
                        path,
                        classification,
                    },
                )
                .collect(),
            candidates: self
                .candidates
                .into_iter()
                .map(|(name, candidate)| PrivateCollisionCandidate {
                    name,
                    slots: candidate.slots.into_iter().map(map_slot).collect(),
                    global_object_contributor: candidate.global_object_contributor,
                })
                .collect(),
            reasons: self.reasons,
            relative_import_edges: self.relative_import_edges,
        };
        (self.route, self.capability.take(), receipt)
    }

    #[cfg(test)]
    fn take_capability(mut self) -> CollisionFreeUserDeltaCapability {
        self.capability
            .take()
            .expect("shared preflight issues its capability")
    }
}

pub(super) enum RoutedProjectPreflight {
    Shared(CollisionFreeUserDeltaCapability),
    Private(PrivateCollisionRouteReceipt),
    UserParseRejected,
    Rejected { reasons: BTreeSet<String> },
}

pub(super) fn preflight_file_inputs(
    root_names: &BTreeSet<String>,
    inputs: &[crate::frontend::FileInput],
) -> RoutedProjectPreflight {
    let inputs = inputs
        .iter()
        .map(|input| PreflightInput {
            path: &input.name,
            source: &input.source,
        })
        .collect::<Vec<_>>();
    let (route, capability, receipt) = preflight_project(root_names, &inputs, false).take_route();
    match route {
        CollisionRoute::SharedDelta => match capability {
            Some(capability) => RoutedProjectPreflight::Shared(capability),
            None => RoutedProjectPreflight::Rejected {
                reasons: BTreeSet::from(["missing-shared-capability".to_owned()]),
            },
        },
        CollisionRoute::PrivateCombined => RoutedProjectPreflight::Private(receipt),
        CollisionRoute::RejectedBeforeSemantics => RoutedProjectPreflight::UserParseRejected,
    }
}

#[cfg(test)]
pub(super) fn preflight_file_inputs_with_omitted_candidate_for_test(
    root_names: &BTreeSet<String>,
    inputs: &[crate::frontend::FileInput],
    omitted_name: &str,
) -> (RoutedProjectPreflight, bool) {
    let authoritative = inputs
        .iter()
        .map(|input| PreflightInput {
            path: &input.name,
            source: &input.source,
        })
        .collect::<Vec<_>>();
    let (_, _, mut receipt) = preflight_project(root_names, &authoritative, false).take_route();
    let omitted = receipt
        .candidates
        .iter()
        .position(|candidate| candidate.name == omitted_name)
        .map(|index| receipt.candidates.remove(index));
    let guard_fired = omitted
        .as_ref()
        .is_some_and(|candidate| root_names.contains(&candidate.name));
    (
        if guard_fired {
            RoutedProjectPreflight::Private(
                preflight_project(root_names, &authoritative, false)
                    .take_route()
                    .2,
            )
        } else {
            RoutedProjectPreflight::Rejected {
                reasons: BTreeSet::from(["candidate-omission-control-did-not-fire".to_owned()]),
            }
        },
        guard_fired,
    )
}

struct PreflightInput<'source> {
    path: &'source str,
    source: &'source str,
}

fn preflight_project(
    root_names: &BTreeSet<String>,
    inputs: &[PreflightInput<'_>],
    inject_uncertainty: bool,
) -> CollisionPreflightOutcome {
    let allocators = (0..inputs.len())
        .map(|_| Allocator::default())
        .collect::<Vec<_>>();
    let mut candidates = BTreeMap::new();
    let mut reasons = BTreeSet::new();
    let mut module_classifications = Vec::with_capacity(inputs.len());
    let mut relative_import_edges = 0_usize;
    let mut work = CollisionPreflightWork::default();
    let parsed = inputs
        .iter()
        .zip(&allocators)
        .map(|(input, allocator)| {
            work.parse_units = work.parse_units.saturating_add(1);
            Parser::new(allocator, input.source, SourceType::ts()).parse()
        })
        .collect::<Vec<_>>();
    let rejected = parsed
        .iter()
        .any(|parsed| parsed.panicked || !parsed.diagnostics.is_empty());
    if rejected {
        reasons.insert("parse-rejected".to_owned());
    }

    let parsed_for_census: &[_] = if rejected { &[] } else { &parsed };
    for (input, parsed) in inputs.iter().zip(parsed_for_census) {
        let context =
            ModuleBindingContext::for_program(&parsed.program, source_file_kind(input.path));
        let classification = if context.external_module {
            PrivateCollisionModuleClassification::External
        } else {
            PrivateCollisionModuleClassification::Script
        };
        module_classifications.push((input.path.to_owned(), classification));
        relative_import_edges = relative_import_edges
            .saturating_add(crate::frontend::relative_import_edge_count(&parsed.program));
        let census = source_global_binding_census(&parsed.program, context);
        work.source_nodes_visited = work
            .source_nodes_visited
            .saturating_add(census.source_nodes_visited);
        work.binding_leaves_visited = work
            .binding_leaves_visited
            .saturating_add(census.binding_leaves_visited);
        for (name, candidate) in census
            .candidates
            .into_iter()
            .chain(census.uncertain_candidates)
        {
            let aggregate =
                candidates
                    .entry(name)
                    .or_insert_with(|| SourceGlobalBindingCandidate {
                        slots: BTreeSet::new(),
                        global_object_contributor: false,
                    });
            aggregate.slots.extend(candidate.slots);
            aggregate.global_object_contributor |= candidate.global_object_contributor;
        }
        if census.explicit_global_this {
            reasons.insert("explicit-global-this".to_owned());
        }
        if census.umd_global {
            reasons.insert("umd-global".to_owned());
        }
        if census.uncertain_relevant_syntax {
            reasons.insert("classifier-uncertainty".to_owned());
        }
    }

    if inject_uncertainty {
        reasons.insert("classifier-uncertainty".to_owned());
    }
    for (name, candidate) in &candidates {
        work.frozen_name_probes = work.frozen_name_probes.saturating_add(1);
        if root_names.contains(name) {
            reasons.insert("frozen-root-name-collision".to_owned());
        }
        if candidate.global_object_contributor {
            reasons.insert("global-object-contributor".to_owned());
        }
    }
    module_classifications.sort_by(|left, right| left.0.cmp(&right.0));
    let route = if rejected {
        CollisionRoute::RejectedBeforeSemantics
    } else if reasons.is_empty() {
        CollisionRoute::SharedDelta
    } else {
        CollisionRoute::PrivateCombined
    };
    let capability =
        (route == CollisionRoute::SharedDelta).then(CollisionFreeUserDeltaCapability::issue);
    CollisionPreflightOutcome {
        route,
        capability,
        module_classifications,
        candidates,
        reasons,
        relative_import_edges,
        #[cfg(test)]
        work,
    }
}

fn map_slot(slot: SourceBindingSlot) -> PrivateCollisionSlot {
    match slot {
        SourceBindingSlot::Value => PrivateCollisionSlot::Value,
        SourceBindingSlot::Type => PrivateCollisionSlot::Type,
        SourceBindingSlot::Namespace => PrivateCollisionSlot::Namespace,
    }
}

#[cfg(test)]
pub(super) fn preflight_for_test(
    root_names: &BTreeSet<String>,
    inputs: &[super::base::UserDeltaProjectInputForTest<'_>],
    inject_uncertainty: bool,
) -> super::base::CollisionPreflightReceiptForTest {
    use super::base::{
        CollisionPreflightReceiptForTest, CollisionPreflightWorkForTest, CollisionRouteForTest,
    };

    let inputs = inputs
        .iter()
        .map(|input| PreflightInput {
            path: input.path,
            source: input.source,
        })
        .collect::<Vec<_>>();
    let outcome = preflight_project(root_names, &inputs, inject_uncertainty);
    let capability_issued = outcome.capability.is_some();
    let route = outcome.route;
    let work = outcome.work.clone();
    let (_, _, receipt) = outcome.take_route();
    CollisionPreflightReceiptForTest {
        route: match route {
            CollisionRoute::SharedDelta => CollisionRouteForTest::SharedDelta,
            CollisionRoute::PrivateCombined => CollisionRouteForTest::PrivateCombined,
            CollisionRoute::RejectedBeforeSemantics => {
                CollisionRouteForTest::RejectedBeforeSemantics
            }
        },
        capability_issued,
        module_classifications: receipt.module_classifications,
        candidates: receipt.candidates,
        reasons: receipt.reasons,
        work: CollisionPreflightWorkForTest {
            parse_units: work.parse_units,
            source_nodes_visited: work.source_nodes_visited,
            binding_leaves_visited: work.binding_leaves_visited,
            frozen_name_probes: work.frozen_name_probes,
            ..CollisionPreflightWorkForTest::default()
        },
    }
}

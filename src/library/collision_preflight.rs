//! Source-only routing preflight for frozen-library user checks.

#![cfg_attr(
    not(test),
    allow(dead_code, reason = "activated by the WU5 private-route cutover")
)]

#[cfg(test)]
use crate::binder::declaration::SourceBindingSlot;
use crate::binder::declaration::{source_global_binding_census, SourceGlobalBindingCandidate};
use crate::binder::namespace::{source_file_kind, ModuleBindingContext};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use std::collections::{BTreeMap, BTreeSet};

/// Sealed authority for one frozen-prefix fork.
pub(crate) struct CollisionFreeUserDeltaCapability(());

impl CollisionFreeUserDeltaCapability {
    fn issue() -> Self {
        CollisionFreeUserDeltaCapability(())
    }
}

#[cfg(test)]
pub(super) fn issue_caller_certified_capability_for_test() -> CollisionFreeUserDeltaCapability {
    preflight_project(&BTreeSet::new(), &[], false).take_capability()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModuleClassification {
    Script,
    External,
}

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
    module_classifications: Vec<(String, ModuleClassification)>,
    candidates: BTreeMap<String, SourceGlobalBindingCandidate>,
    reasons: BTreeSet<String>,
    work: CollisionPreflightWork,
}

impl CollisionPreflightOutcome {
    #[cfg(test)]
    fn take_capability(mut self) -> CollisionFreeUserDeltaCapability {
        self.capability
            .take()
            .expect("shared preflight issues its capability")
    }
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
    let mut work = CollisionPreflightWork::default();
    let mut rejected = false;

    for (input, allocator) in inputs.iter().zip(&allocators) {
        let parsed = Parser::new(allocator, input.source, SourceType::ts()).parse();
        work.parse_units = work.parse_units.saturating_add(1);
        let context =
            ModuleBindingContext::for_program(&parsed.program, source_file_kind(input.path));
        let classification = if context.external_module {
            ModuleClassification::External
        } else {
            ModuleClassification::Script
        };
        module_classifications.push((input.path.to_owned(), classification));

        if parsed.panicked {
            rejected = true;
            reasons.insert("parse-rejected".to_owned());
            continue;
        }
        if !parsed.diagnostics.is_empty() {
            reasons.insert("classifier-uncertainty".to_owned());
        }
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
        work,
    }
}

#[cfg(test)]
pub(super) fn preflight_for_test(
    root_names: &BTreeSet<String>,
    inputs: &[super::base::UserDeltaProjectInputForTest<'_>],
    inject_uncertainty: bool,
) -> super::base::CollisionPreflightReceiptForTest {
    use super::base::{
        CollisionCandidateForTest, CollisionPreflightReceiptForTest, CollisionPreflightWorkForTest,
        CollisionRouteForTest, ModuleClassificationEntryForTest, ModuleClassificationForTest,
        PreflightSlotForTest,
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
    CollisionPreflightReceiptForTest {
        route: match outcome.route {
            CollisionRoute::SharedDelta => CollisionRouteForTest::SharedDelta,
            CollisionRoute::PrivateCombined => CollisionRouteForTest::PrivateCombined,
            CollisionRoute::RejectedBeforeSemantics => {
                CollisionRouteForTest::RejectedBeforeSemantics
            }
        },
        capability_issued,
        module_classifications: outcome
            .module_classifications
            .into_iter()
            .map(|(path, classification)| {
                let classification = match classification {
                    ModuleClassification::Script => ModuleClassificationForTest::Script,
                    ModuleClassification::External => ModuleClassificationForTest::External,
                };
                ModuleClassificationEntryForTest {
                    path,
                    classification,
                }
            })
            .collect(),
        candidates: outcome
            .candidates
            .into_iter()
            .map(|(name, candidate)| CollisionCandidateForTest {
                name,
                slots: candidate
                    .slots
                    .into_iter()
                    .map(|slot| match slot {
                        SourceBindingSlot::Value => PreflightSlotForTest::Value,
                        SourceBindingSlot::Type => PreflightSlotForTest::Type,
                        SourceBindingSlot::Namespace => PreflightSlotForTest::Namespace,
                    })
                    .collect(),
                global_object_contributor: candidate.global_object_contributor,
            })
            .collect(),
        reasons: outcome.reasons,
        work: CollisionPreflightWorkForTest {
            parse_units: outcome.work.parse_units,
            source_nodes_visited: outcome.work.source_nodes_visited,
            binding_leaves_visited: outcome.work.binding_leaves_visited,
            frozen_name_probes: outcome.work.frozen_name_probes,
            ..CollisionPreflightWorkForTest::default()
        },
    }
}

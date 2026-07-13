//! Shared immutable domain for ADR-0006 class publication and semantic outcomes.

use crate::types::repr::ClassId;
use rustc_hash::FxHashMap;

/// Construction state for one class declaration.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum ClassConstructionState {
    Pending,
    Building,
    Built,
    Published,
    Poisoned,
}

/// Typed reason a semantic demand could not safely complete.
#[allow(clippy::enum_variant_names)] // ADR-0006 fixes these externally meaningful reason names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Exhaustion {
    ClassNotPublished {
        class: ClassId,
        state: ClassConstructionState,
    },
    ClassHeritagePoison {
        class: ClassId,
    },
    ClassProjectionBudget,
}

/// Evaluation/projection outcome. Exhaustion is never folded into recovery.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DemandOutcome<T> {
    Ready(T),
    Exhausted(Exhaustion),
}

/// Immutable proof that every registered class reached a final state.
pub(crate) struct PublishedClasses {
    states: FxHashMap<ClassId, ClassConstructionState>,
}

impl PublishedClasses {
    pub(crate) fn from_final_states(
        states: FxHashMap<ClassId, ClassConstructionState>,
    ) -> Option<Self> {
        states
            .values()
            .all(|state| {
                matches!(
                    state,
                    ClassConstructionState::Published | ClassConstructionState::Poisoned
                )
            })
            .then_some(PublishedClasses { states })
    }

    pub(crate) fn empty() -> Self {
        PublishedClasses {
            states: FxHashMap::default(),
        }
    }

    pub(crate) fn require(&self, class: ClassId) -> DemandOutcome<()> {
        match self.states.get(&class).copied() {
            Some(ClassConstructionState::Published) => DemandOutcome::Ready(()),
            Some(ClassConstructionState::Poisoned) => {
                DemandOutcome::Exhausted(Exhaustion::ClassHeritagePoison { class })
            }
            Some(state) => DemandOutcome::Exhausted(Exhaustion::ClassNotPublished { class, state }),
            None => DemandOutcome::Exhausted(Exhaustion::ClassNotPublished {
                class,
                state: ClassConstructionState::Pending,
            }),
        }
    }

    #[cfg(test)]
    pub(crate) fn forged(class: ClassId, state: ClassConstructionState) -> Self {
        PublishedClasses {
            states: FxHashMap::from_iter([(class, state)]),
        }
    }

    #[cfg(test)]
    pub(crate) fn forged_states(
        states: impl IntoIterator<Item = (ClassId, ClassConstructionState)>,
    ) -> Self {
        PublishedClasses {
            states: FxHashMap::from_iter(states),
        }
    }
}

pub(crate) fn is_prepublication(reason: &Exhaustion) -> bool {
    matches!(reason, Exhaustion::ClassNotPublished { .. })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demand_outcomes_require_exhaustive_matching() {
        fn classify(value: DemandOutcome<()>) -> u8 {
            match value {
                DemandOutcome::Ready(()) => 0,
                DemandOutcome::Exhausted(_) => 1,
            }
        }

        assert_eq!(classify(DemandOutcome::Ready(())), 0);
        assert_eq!(
            classify(DemandOutcome::Exhausted(Exhaustion::ClassProjectionBudget)),
            1
        );
    }
}

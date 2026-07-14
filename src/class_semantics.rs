//! Shared immutable domain for ADR-0006 class publication and semantic outcomes.

use crate::types::repr::{ClassId, TypeParamId};
use crate::types::store::TypeId;
use rustc_hash::FxHashMap;

/// Construction state for one class declaration.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum ClassConstructionState {
    Pending,
    #[allow(dead_code)] // Kept for the ADR-0006 publication-state contract.
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
    ClassInitializerPoison {
        class: ClassId,
    },
    ClassSurfacePoison {
        class: ClassId,
    },
    ClassApplicationArguments(ClassApplicationArguments),
    ClassProjectionBudget,
    EvaluationBudget,
    EvaluationCycle {
        ty: TypeId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClassDefaultDeclaration {
    pub(crate) class: ClassId,
    pub(crate) parameter: TypeParamId,
    pub(crate) index: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ClassApplicationArguments {
    WrongArity {
        expected_min: usize,
        expected_max: usize,
        actual: usize,
    },
    UnavailableExplicitArgument {
        index: usize,
    },
    UnsupportedDefault {
        declaration: ClassDefaultDeclaration,
    },
    InferenceIncomplete {
        index: usize,
    },
    TargetPoisoned {
        class: ClassId,
    },
}

/// Evaluation/projection outcome. Exhaustion is never folded into recovery.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DemandOutcome<T> {
    Ready(T),
    Exhausted(Exhaustion),
}

/// Immutable proof that every registered class reached a final state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PublishedClassSurface {
    class: ClassId,
    type_params: Box<[TypeParamId]>,
    instance_template: TypeId,
    static_template: TypeId,
    constructor_template: Option<TypeId>,
}

impl PublishedClassSurface {
    pub(crate) fn new(
        class: ClassId,
        type_params: Vec<TypeParamId>,
        instance_template: TypeId,
        static_template: TypeId,
        constructor_template: Option<TypeId>,
    ) -> Self {
        PublishedClassSurface {
            class,
            type_params: type_params.into_boxed_slice(),
            instance_template,
            static_template,
            constructor_template,
        }
    }

    pub(crate) fn class(&self) -> ClassId {
        self.class
    }

    pub(crate) fn type_params(&self) -> &[TypeParamId] {
        &self.type_params
    }

    pub(crate) fn instance_template(&self) -> TypeId {
        self.instance_template
    }

    pub(crate) fn static_template(&self) -> TypeId {
        self.static_template
    }

    pub(crate) fn constructor_template(&self) -> Option<TypeId> {
        self.constructor_template
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum PublishedClassPoison {
    Heritage,
    Initializer,
    Surface,
}

/// Immutable proof that every registered class reached a final state. Drafts
/// and partially composed surfaces never enter this registry.
pub(crate) struct PublishedClasses {
    states: FxHashMap<ClassId, ClassConstructionState>,
    surfaces: FxHashMap<ClassId, PublishedClassSurface>,
    poison: FxHashMap<ClassId, PublishedClassPoison>,
}

impl PublishedClasses {
    pub(crate) fn from_publication(
        states: FxHashMap<ClassId, ClassConstructionState>,
        surfaces: FxHashMap<ClassId, PublishedClassSurface>,
        poison: FxHashMap<ClassId, PublishedClassPoison>,
    ) -> Option<Self> {
        let every_final = states.values().all(|state| {
            matches!(
                state,
                ClassConstructionState::Published | ClassConstructionState::Poisoned
            )
        });
        let every_published_has_surface = states.iter().all(|(class, state)| {
            *state != ClassConstructionState::Published || surfaces.contains_key(class)
        });
        let every_poisoned_has_cause = states.iter().all(|(class, state)| {
            *state != ClassConstructionState::Poisoned || poison.contains_key(class)
        });
        let exact_surface_set = surfaces.len()
            == states
                .values()
                .filter(|state| **state == ClassConstructionState::Published)
                .count()
            && surfaces.iter().all(|(class, surface)| {
                states.get(class) == Some(&ClassConstructionState::Published)
                    && surface.class() == *class
            });
        let exact_poison_set = poison.len()
            == states
                .values()
                .filter(|state| **state == ClassConstructionState::Poisoned)
                .count()
            && poison
                .keys()
                .all(|class| states.get(class) == Some(&ClassConstructionState::Poisoned));
        (every_final
            && every_published_has_surface
            && every_poisoned_has_cause
            && exact_surface_set
            && exact_poison_set)
            .then_some(PublishedClasses {
                states,
                surfaces,
                poison,
            })
    }

    pub(crate) fn empty() -> Self {
        PublishedClasses {
            states: FxHashMap::default(),
            surfaces: FxHashMap::default(),
            poison: FxHashMap::default(),
        }
    }

    pub(crate) fn require(&self, class: ClassId) -> DemandOutcome<()> {
        match self.states.get(&class).copied() {
            Some(ClassConstructionState::Published) => DemandOutcome::Ready(()),
            Some(ClassConstructionState::Poisoned) => match self.poison.get(&class) {
                Some(PublishedClassPoison::Initializer) => {
                    DemandOutcome::Exhausted(Exhaustion::ClassInitializerPoison { class })
                }
                Some(PublishedClassPoison::Surface) => {
                    DemandOutcome::Exhausted(Exhaustion::ClassSurfacePoison { class })
                }
                Some(PublishedClassPoison::Heritage) | None => {
                    DemandOutcome::Exhausted(Exhaustion::ClassHeritagePoison { class })
                }
            },
            Some(state) => DemandOutcome::Exhausted(Exhaustion::ClassNotPublished { class, state }),
            None => DemandOutcome::Exhausted(Exhaustion::ClassNotPublished {
                class,
                state: ClassConstructionState::Pending,
            }),
        }
    }

    pub(crate) fn published_class(&self, class: ClassId) -> DemandOutcome<&PublishedClassSurface> {
        match self.require(class) {
            DemandOutcome::Ready(()) => match self.surfaces.get(&class) {
                Some(surface) => DemandOutcome::Ready(surface),
                None => DemandOutcome::Exhausted(Exhaustion::ClassNotPublished {
                    class,
                    state: ClassConstructionState::Built,
                }),
            },
            DemandOutcome::Exhausted(reason) => DemandOutcome::Exhausted(reason),
        }
    }

    #[cfg(test)]
    pub(crate) fn forged(class: ClassId, state: ClassConstructionState) -> Self {
        PublishedClasses {
            states: FxHashMap::from_iter([(class, state)]),
            surfaces: FxHashMap::default(),
            poison: FxHashMap::default(),
        }
    }
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

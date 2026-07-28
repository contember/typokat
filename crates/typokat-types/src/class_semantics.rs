//! Shared immutable domain for ADR-0006 class publication and semantic outcomes.

use crate::types::layered::LayeredMap;
use crate::types::repr::{ClassId, TypeParamId};
use crate::types::store::TypeId;
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Construction state for one class declaration.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ClassConstructionState {
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
pub enum Exhaustion {
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
pub struct ClassDefaultDeclaration {
    pub class: ClassId,
    pub parameter: TypeParamId,
    pub index: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClassApplicationArguments {
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
pub enum DemandOutcome<T> {
    Ready(T),
    Exhausted(Exhaustion),
}

/// Immutable proof that every registered class reached a final state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublishedClassSurface {
    class: ClassId,
    type_params: Box<[TypeParamId]>,
    instance_template: TypeId,
    static_template: TypeId,
    constructor_template: Option<TypeId>,
}

impl PublishedClassSurface {
    pub fn new(
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

    pub fn class(&self) -> ClassId {
        self.class
    }

    pub fn type_params(&self) -> &[TypeParamId] {
        &self.type_params
    }

    pub fn instance_template(&self) -> TypeId {
        self.instance_template
    }

    pub fn static_template(&self) -> TypeId {
        self.static_template
    }

    pub fn constructor_template(&self) -> Option<TypeId> {
        self.constructor_template
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PublishedClassPoison {
    Heritage,
    Initializer,
    Surface,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CanonicalPublishedClassTerminal<'a> {
    Ready(&'a PublishedClassSurface),
    HeritagePoison,
    InitializerPoison,
    SurfacePoison,
}

/// Lifetime-free class publication row: the owned form of
/// [`CanonicalPublishedClassTerminal`], used when the frozen product is decomposed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnedPublishedClassTerminal {
    Ready(PublishedClassSurface),
    Poisoned(PublishedClassPoison),
}

/// Immutable proof that every registered class reached a final state. Drafts
/// and partially composed surfaces never enter this registry.
pub struct PublishedClasses {
    states: LayeredMap<ClassId, ClassConstructionState>,
    surfaces: LayeredMap<ClassId, PublishedClassSurface>,
    poison: LayeredMap<ClassId, PublishedClassPoison>,
    identity: Arc<()>,
}

impl Clone for PublishedClasses {
    fn clone(&self) -> Self {
        Self {
            states: self.states.clone(),
            surfaces: self.surfaces.clone(),
            poison: self.poison.clone(),
            identity: Arc::clone(&self.identity),
        }
    }
}

impl PublishedClasses {
    pub fn owned_terminals(&self) -> Option<Vec<(ClassId, OwnedPublishedClassTerminal)>> {
        self.canonical_terminals().map(|terminals| {
            terminals
                .into_iter()
                .map(|(class, terminal)| {
                    let terminal = match terminal {
                        CanonicalPublishedClassTerminal::Ready(surface) => {
                            OwnedPublishedClassTerminal::Ready(surface.clone())
                        }
                        CanonicalPublishedClassTerminal::HeritagePoison => {
                            OwnedPublishedClassTerminal::Poisoned(PublishedClassPoison::Heritage)
                        }
                        CanonicalPublishedClassTerminal::InitializerPoison => {
                            OwnedPublishedClassTerminal::Poisoned(PublishedClassPoison::Initializer)
                        }
                        CanonicalPublishedClassTerminal::SurfacePoison => {
                            OwnedPublishedClassTerminal::Poisoned(PublishedClassPoison::Surface)
                        }
                    };
                    (class, terminal)
                })
                .collect()
        })
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn local_owned_terminals(&self) -> Vec<(ClassId, OwnedPublishedClassTerminal)> {
        self.states
            .local_iter()
            .filter_map(|(&class, state)| match state {
                ClassConstructionState::Published => self
                    .surfaces
                    .get(&class)
                    .cloned()
                    .map(|surface| (class, OwnedPublishedClassTerminal::Ready(surface))),
                ClassConstructionState::Poisoned => self
                    .poison
                    .get(&class)
                    .cloned()
                    .map(|cause| (class, OwnedPublishedClassTerminal::Poisoned(cause))),
                ClassConstructionState::Pending
                | ClassConstructionState::Building
                | ClassConstructionState::Built => None,
            })
            .collect()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn local_row_count_for_test(&self) -> usize {
        self.states.local_len() + self.surfaces.local_len() + self.poison.local_len()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn from_owned_terminals(
        terminals: Vec<(ClassId, OwnedPublishedClassTerminal)>,
    ) -> Result<Self, &'static str> {
        if terminals.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
            return Err("owned class terminals are not strictly ordered");
        }
        let mut states = FxHashMap::default();
        let mut surfaces = FxHashMap::default();
        let mut poison = FxHashMap::default();
        for (class, terminal) in terminals {
            match terminal {
                OwnedPublishedClassTerminal::Ready(surface) => {
                    if surface.class() != class {
                        return Err("owned class surface owns a different class id");
                    }
                    states.insert(class, ClassConstructionState::Published);
                    surfaces.insert(class, surface);
                }
                OwnedPublishedClassTerminal::Poisoned(cause) => {
                    states.insert(class, ClassConstructionState::Poisoned);
                    poison.insert(class, cause);
                }
            }
        }
        Self::from_publication(states, surfaces, poison)
            .ok_or("owned class publication is not terminal")
    }

    pub fn canonical_terminals(
        &self,
    ) -> Option<Vec<(ClassId, CanonicalPublishedClassTerminal<'_>)>> {
        let mut classes = self.states.keys().copied().collect::<Vec<_>>();
        classes.sort_by_key(|class| class.0);
        classes
            .into_iter()
            .map(|class| {
                let terminal = match self.states.get(&class)? {
                    ClassConstructionState::Published => {
                        CanonicalPublishedClassTerminal::Ready(self.surfaces.get(&class)?)
                    }
                    ClassConstructionState::Poisoned => match self.poison.get(&class)? {
                        PublishedClassPoison::Heritage => {
                            CanonicalPublishedClassTerminal::HeritagePoison
                        }
                        PublishedClassPoison::Initializer => {
                            CanonicalPublishedClassTerminal::InitializerPoison
                        }
                        PublishedClassPoison::Surface => {
                            CanonicalPublishedClassTerminal::SurfacePoison
                        }
                    },
                    ClassConstructionState::Pending
                    | ClassConstructionState::Building
                    | ClassConstructionState::Built => return None,
                };
                Some((class, terminal))
            })
            .collect()
    }

    pub fn extend(mut self, extension: Self) -> Option<Self> {
        if extension
            .states
            .keys()
            .any(|class| self.states.contains_key(class))
            || extension
                .surfaces
                .keys()
                .any(|class| self.surfaces.contains_key(class))
            || extension
                .poison
                .keys()
                .any(|class| self.poison.contains_key(class))
        {
            return None;
        }
        for (&class, &state) in extension.states.iter() {
            self.states.insert_local(class, state).ok()?;
        }
        for (&class, surface) in extension.surfaces.iter() {
            self.surfaces.insert_local(class, surface.clone()).ok()?;
        }
        for (&class, &poison) in extension.poison.iter() {
            self.poison.insert_local(class, poison).ok()?;
        }
        self.identity = Arc::new(());
        Some(self)
    }

    pub fn from_publication(
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
                states: states.into(),
                surfaces: surfaces.into(),
                poison: poison.into(),
                identity: Arc::new(()),
            })
    }

    pub fn empty() -> Self {
        PublishedClasses {
            states: LayeredMap::default(),
            surfaces: LayeredMap::default(),
            poison: LayeredMap::default(),
            identity: Arc::new(()),
        }
    }

    pub fn identity(&self) -> &Arc<()> {
        &self.identity
    }

    pub fn freeze_as_base(&mut self) -> Result<(), &'static str> {
        self.states.freeze_as_base()?;
        self.surfaces.freeze_as_base()?;
        self.poison.freeze_as_base()
    }

    pub fn fork_delta(&self) -> Result<Self, &'static str> {
        Ok(Self {
            states: self.states.fork_delta()?,
            surfaces: self.surfaces.fork_delta()?,
            poison: self.poison.fork_delta()?,
            identity: Arc::clone(&self.identity),
        })
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn shares_base_with(&self, other: &Self) -> bool {
        self.states.shares_base_with(&other.states)
            && self.surfaces.shares_base_with(&other.surfaces)
            && self.poison.shares_base_with(&other.poison)
    }

    pub fn require(&self, class: ClassId) -> DemandOutcome<()> {
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

    pub fn published_class(&self, class: ClassId) -> DemandOutcome<&PublishedClassSurface> {
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

    #[cfg(any(test, feature = "test-utils"))]
    pub fn forged(class: ClassId, state: ClassConstructionState) -> Self {
        PublishedClasses {
            states: FxHashMap::from_iter([(class, state)]).into(),
            surfaces: LayeredMap::default(),
            poison: LayeredMap::default(),
            identity: Arc::new(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_class_terminals_round_trip_exactly() {
        let ready = ClassId(2);
        let poisoned = ClassId(7);
        let surface = PublishedClassSurface::new(
            ready,
            vec![TypeParamId(11)],
            TypeId(13),
            TypeId(17),
            Some(TypeId(19)),
        );
        let publication = PublishedClasses::from_publication(
            FxHashMap::from_iter([
                (ready, ClassConstructionState::Published),
                (poisoned, ClassConstructionState::Poisoned),
            ]),
            FxHashMap::from_iter([(ready, surface)]),
            FxHashMap::from_iter([(poisoned, PublishedClassPoison::Initializer)]),
        )
        .expect("terminal publication");

        let parts = publication.owned_terminals().expect("owned terminals");
        let restored =
            PublishedClasses::from_owned_terminals(parts.clone()).expect("restore terminals");

        assert_eq!(restored.owned_terminals(), Some(parts));
    }

    #[test]
    fn owned_class_terminals_reject_unordered_and_mismatched_rows() {
        let surface =
            PublishedClassSurface::new(ClassId(3), Vec::new(), TypeId(5), TypeId(7), None);
        assert!(PublishedClasses::from_owned_terminals(vec![
            (
                ClassId(2),
                OwnedPublishedClassTerminal::Poisoned(PublishedClassPoison::Surface),
            ),
            (
                ClassId(1),
                OwnedPublishedClassTerminal::Ready(surface.clone())
            ),
        ])
        .is_err());
        assert!(PublishedClasses::from_owned_terminals(vec![(
            ClassId(4),
            OwnedPublishedClassTerminal::Ready(surface),
        )])
        .is_err());
    }

    #[test]
    fn published_classes_share_frozen_rows_and_isolate_extensions() {
        let base_class = ClassId(2);
        let local_class = ClassId(3);
        let mut base = PublishedClasses::from_publication(
            FxHashMap::from_iter([(base_class, ClassConstructionState::Published)]),
            FxHashMap::from_iter([(
                base_class,
                PublishedClassSurface::new(base_class, Vec::new(), TypeId(10), TypeId(11), None),
            )]),
            FxHashMap::default(),
        )
        .expect("base publication");
        base.freeze_as_base().expect("class base seals");
        let first = base.fork_delta().expect("first class suffix");
        let second = base.fork_delta().expect("second class suffix");
        assert!(first.shares_base_with(&second));

        let extension = PublishedClasses::from_publication(
            FxHashMap::from_iter([(local_class, ClassConstructionState::Published)]),
            FxHashMap::from_iter([(
                local_class,
                PublishedClassSurface::new(local_class, Vec::new(), TypeId(12), TypeId(13), None),
            )]),
            FxHashMap::default(),
        )
        .expect("local publication");
        let extended = first.extend(extension).expect("disjoint class suffix");
        assert!(matches!(
            extended.require(local_class),
            DemandOutcome::Ready(())
        ));
        assert!(matches!(
            second.require(local_class),
            DemandOutcome::Exhausted(Exhaustion::ClassNotPublished { .. })
        ));
        assert!(base.shares_base_with(&extended));
    }

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

//! Checker-owned mutable construction capability for ADR-0006 class surfaces.

use crate::class_semantics::{
    is_prepublication, ClassConstructionState, DemandOutcome, PublishedClasses,
};
use crate::types::repr::ClassId;
use crate::types::store::{Store, TypeId};
use crate::types::Interner;
use rustc_hash::FxHashMap;

#[derive(Default)]
pub(in crate::check::checker) struct ClassConstruction {
    states: FxHashMap<ClassId, ClassConstructionState>,
}

impl ClassConstruction {
    /// Register only a vacant class identity. Existing states are never reset.
    pub(in crate::check::checker) fn register(&mut self, class: ClassId) -> bool {
        use std::collections::hash_map::Entry;

        match self.states.entry(class) {
            Entry::Vacant(entry) => {
                entry.insert(ClassConstructionState::Pending);
                true
            }
            Entry::Occupied(_) => false,
        }
    }

    pub(in crate::check::checker) fn state(
        &self,
        class: ClassId,
    ) -> Option<ClassConstructionState> {
        self.states.get(&class).copied()
    }

    pub(in crate::check::checker) fn begin_surface<'a>(
        &'a mut self,
        class: ClassId,
        interner: &'a mut Interner,
    ) -> Option<ClassSurfaceBuilder<'a>> {
        let state = self.states.get_mut(&class)?;
        if *state != ClassConstructionState::Pending {
            return None;
        }
        *state = ClassConstructionState::Building;
        Some(ClassSurfaceBuilder {
            class,
            state,
            interner,
        })
    }

    pub(in crate::check::checker) fn poison(&mut self, class: ClassId) -> bool {
        let Some(state) = self.states.get_mut(&class) else {
            return false;
        };
        if !matches!(
            *state,
            ClassConstructionState::Pending
                | ClassConstructionState::Building
                | ClassConstructionState::Built
        ) {
            return false;
        }
        *state = ClassConstructionState::Poisoned;
        true
    }

    /// Publish a complete set atomically. Any non-built member rejects the set.
    pub(in crate::check::checker) fn publish(&mut self, classes: &[ClassId]) -> bool {
        if classes
            .iter()
            .any(|class| self.states.get(class).copied() != Some(ClassConstructionState::Built))
        {
            return false;
        }
        for class in classes {
            if let Some(state) = self.states.get_mut(class) {
                *state = ClassConstructionState::Published;
            }
        }
        true
    }

    pub(in crate::check::checker) fn finish(self) -> Option<PublishedClasses> {
        PublishedClasses::from_final_states(self.states)
    }
}

/// Construction capability with no evaluator, projector, or relater access.
pub(in crate::check::checker) struct ClassSurfaceBuilder<'a> {
    class: ClassId,
    state: &'a mut ClassConstructionState,
    interner: &'a mut Interner,
}

impl ClassSurfaceBuilder<'_> {
    pub(in crate::check::checker) fn intern_class_instance(
        &mut self,
        class: ClassId,
        args: Vec<TypeId>,
    ) -> TypeId {
        self.interner.intern_class_instance(class, args)
    }

    pub(in crate::check::checker) fn intern_deferred_indexed_access(
        &mut self,
        object: TypeId,
        index: TypeId,
    ) -> TypeId {
        self.interner.intern_deferred_indexed_access(object, index)
    }

    pub(in crate::check::checker) fn class(&self) -> ClassId {
        self.class
    }

    pub(in crate::check::checker) fn finish(self) {
        *self.state = ClassConstructionState::Built;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct ClassProjectionRequest {
    pub application: TypeId,
    pub class: ClassId,
    pub args: Vec<TypeId>,
}

pub(in crate::check::checker) fn request_class_projection(
    published: &PublishedClasses,
    store: &Store,
    application: TypeId,
) -> DemandOutcome<Option<ClassProjectionRequest>> {
    let Some(instance) = store.class_instance_type(application) else {
        return DemandOutcome::Ready(None);
    };
    match published.require(instance.class) {
        DemandOutcome::Ready(()) => DemandOutcome::Ready(Some(ClassProjectionRequest {
            application,
            class: instance.class,
            args: instance.args.clone(),
        })),
        DemandOutcome::Exhausted(reason) => {
            if is_prepublication(&reason) {
                record_projection_tripwire();
            }
            DemandOutcome::Exhausted(reason)
        }
    }
}

#[cfg(test)]
thread_local! {
    static PROJECTION_TRIPWIRE: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

fn record_projection_tripwire() {
    #[cfg(test)]
    PROJECTION_TRIPWIRE.with(|count| count.set(count.get() + 1));
}

#[cfg(test)]
fn reset_projection_tripwire() {
    PROJECTION_TRIPWIRE.with(|count| count.set(0));
}

#[cfg(test)]
fn projection_tripwire() -> u64 {
    PROJECTION_TRIPWIRE.with(std::cell::Cell::get)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::checker::eval::legacy_guard::{
        evaluation_guard_measure, reset_evaluation_guard_measure, EvaluationGuardMeasure,
    };
    use crate::class_semantics::Exhaustion;
    use crate::relate::relation::legacy_guard::{
        relation_guard_measure, reset_relation_guard_measure, RelationGuardMeasure,
    };

    #[test]
    fn construction_capability_publishes_only_final_states() {
        reset_evaluation_guard_measure();
        reset_projection_tripwire();
        reset_relation_guard_measure();
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let class = ClassId(7);
        let mut construction = ClassConstruction::default();
        assert!(construction.register(class));
        let application;
        {
            let mut builder = construction
                .begin_surface(class, &mut interner)
                .expect("pending class enters building");
            assert_eq!(builder.class(), class);
            application = builder.intern_class_instance(class, vec![wk.number]);
            let _ = builder.intern_deferred_indexed_access(wk.number, wk.string);
            builder.finish();
        }
        assert_eq!(
            construction.state(class),
            Some(ClassConstructionState::Built)
        );
        assert!(construction.publish(&[class]));
        let published = construction.finish().expect("all classes are final");
        assert!(matches!(
            request_class_projection(&published, interner.store(), application),
            DemandOutcome::Ready(Some(ClassProjectionRequest { class: found, .. }))
                if found == class
        ));
        assert_eq!(
            evaluation_guard_measure(),
            EvaluationGuardMeasure::default()
        );
        assert_eq!(projection_tripwire(), 0);
        assert_eq!(relation_guard_measure(), RelationGuardMeasure::default());
    }

    #[test]
    fn duplicate_registration_never_resets_any_state() {
        let class = ClassId(8);
        for state in [
            ClassConstructionState::Pending,
            ClassConstructionState::Building,
            ClassConstructionState::Built,
            ClassConstructionState::Published,
            ClassConstructionState::Poisoned,
        ] {
            let mut construction = ClassConstruction {
                states: FxHashMap::from_iter([(class, state)]),
            };
            assert!(!construction.register(class));
            assert_eq!(construction.state(class), Some(state));
        }
    }

    #[test]
    fn state_transitions_are_monotonic() {
        let mut interner = Interner::with_intrinsics();
        let class = ClassId(9);
        let mut construction = ClassConstruction::default();
        assert!(construction.register(class));
        assert_eq!(
            construction.state(class),
            Some(ClassConstructionState::Pending)
        );
        let builder = construction.begin_surface(class, &mut interner).unwrap();
        builder.finish();
        assert_eq!(
            construction.state(class),
            Some(ClassConstructionState::Built)
        );
        assert!(construction.publish(&[class]));
        assert_eq!(
            construction.state(class),
            Some(ClassConstructionState::Published)
        );
        assert!(!construction.register(class));
        assert!(!construction.poison(class));
        assert_eq!(
            construction.state(class),
            Some(ClassConstructionState::Published)
        );
    }

    #[test]
    fn projection_rejects_prepublication_and_preserves_poison() {
        reset_projection_tripwire();
        let mut interner = Interner::with_intrinsics();
        let class = ClassId(10);
        let application = interner.intern_class_instance(class, Vec::new());
        let len = interner.store().len();

        for state in [
            ClassConstructionState::Pending,
            ClassConstructionState::Building,
            ClassConstructionState::Built,
        ] {
            let published = PublishedClasses::forged(class, state);
            assert!(matches!(
                request_class_projection(&published, interner.store(), application),
                DemandOutcome::Exhausted(Exhaustion::ClassNotPublished {
                    class: found,
                    state: found_state,
                }) if found == class && found_state == state
            ));
        }
        let poisoned = PublishedClasses::forged(class, ClassConstructionState::Poisoned);
        assert!(matches!(
            request_class_projection(&poisoned, interner.store(), application),
            DemandOutcome::Exhausted(Exhaustion::ClassHeritagePoison { class: found })
                if found == class
        ));
        assert_eq!(projection_tripwire(), 3);
        assert_eq!(interner.store().len(), len);
    }
}

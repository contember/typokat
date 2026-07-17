//! Compilation-wide class construction graph and atomic surface publication.

use super::application::{ClassTypeParameter, ClassTypeParameterDefault};
use super::retained::RetainedClassCallable;
use crate::class_semantics::{
    ClassConstructionState, PublishedClassPoison, PublishedClassSurface, PublishedClasses,
};
use crate::source::SourceOrdinal;
use crate::types::repr::{ClassId, FunctionType, ObjectType, TypeParamId, TypeTag};
use crate::types::store::{Store, TypeId, TypeParamFreezeError};
use crate::types::{substitute, Interner};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum ReservedRootKind {
    Alias,
    Interface,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReservedRoot {
    kind: ReservedRootKind,
    children: Box<[TypeId]>,
}

/// Reserved alias/interface templates are traversable identity roots, never SCC
/// nodes. Registration is complete before any class graph is built.
#[derive(Default)]
pub(in crate::check::checker) struct ReservedSurfaceRoots {
    roots: BTreeMap<TypeId, ReservedRoot>,
}

impl ReservedSurfaceRoots {
    pub(in crate::check::checker) fn register(
        &mut self,
        root: TypeId,
        kind: ReservedRootKind,
        children: Vec<TypeId>,
    ) -> bool {
        use std::collections::btree_map::Entry;
        match self.roots.entry(root) {
            Entry::Vacant(entry) => {
                entry.insert(ReservedRoot {
                    kind,
                    children: children.into_boxed_slice(),
                });
                true
            }
            Entry::Occupied(_) => false,
        }
    }

    fn children(&self, root: TypeId) -> Option<&[TypeId]> {
        self.roots.get(&root).map(|root| root.children.as_ref())
    }

    #[cfg(test)]
    fn kind(&self, root: TypeId) -> Option<ReservedRootKind> {
        self.roots.get(&root).map(|root| root.kind)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct DraftClassTypeParameter<Ticket> {
    application: ClassTypeParameter<Ticket>,
    constraint: Option<TypeId>,
}

impl<Ticket> DraftClassTypeParameter<Ticket> {
    /// Source class defaults are deliberately always unavailable in this cutover.
    pub(in crate::check::checker) fn source(
        id: TypeParamId,
        constraint: Option<TypeId>,
        default_owner: Option<Ticket>,
    ) -> Self {
        DraftClassTypeParameter {
            application: ClassTypeParameter {
                id,
                default: match default_owner {
                    Some(owner) => ClassTypeParameterDefault::Unsupported(owner),
                    None => ClassTypeParameterDefault::Absent,
                },
            },
            constraint,
        }
    }

    pub(in crate::check::checker) fn merged(
        id: TypeParamId,
        constraint: Option<TypeId>,
        default: ClassTypeParameterDefault<Ticket>,
    ) -> Self {
        Self {
            application: ClassTypeParameter { id, default },
            constraint,
        }
    }

    pub(in crate::check::checker) fn application(&self) -> &ClassTypeParameter<Ticket> {
        &self.application
    }

    pub(in crate::check::checker) fn constraint(&self) -> Option<TypeId> {
        self.constraint
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct HeritageDependency<Ticket> {
    pub target: ClassId,
    pub identity_root: TypeId,
    pub owner: Ticket,
}

/// Stable source order for asymmetric recovery inside an invalid soft-heritage SCC.
/// It deliberately does not depend on dependency scheduling or `ClassId` allocation.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::check::checker) struct ClassRecoveryOrder {
    pub source: SourceOrdinal,
    pub binding_start: u32,
    pub declaration_ordinal: u32,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct InitializerPoisonOrigin<Ticket> {
    pub owner: Ticket,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct HeritageSurfacePoisonOrigin<Ticket> {
    pub owner: Ticket,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct SurfacePoisonOrigin<Ticket> {
    pub owner: Ticket,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum PendingSurfaceObligation<Ticket> {
    InitializerOrigin {
        class: ClassId,
        owner: Ticket,
    },
    SurfaceOrigin {
        class: ClassId,
        owner: Ticket,
    },
    HeritageCycle {
        derived: ClassId,
        base: ClassId,
        owner: Ticket,
    },
    PoisonedBase {
        derived: ClassId,
        base: ClassId,
        owner: Ticket,
    },
    Deferred {
        class: ClassId,
        owner: Ticket,
    },
}

/// Complete query-free draft for one registered class declaration.
#[derive(Clone, Debug)]
pub(in crate::check::checker) struct ClassDraft<Ticket: Copy> {
    pub class: ClassId,
    pub recovery_order: ClassRecoveryOrder,
    pub type_parameters: Vec<DraftClassTypeParameter<Ticket>>,
    pub instance_template: TypeId,
    pub static_template: TypeId,
    pub constructor_template: Option<TypeId>,
    pub ordinary_identity_roots: Vec<TypeId>,
    pub heritage: Option<HeritageDependency<Ticket>>,
    pub instance_heritage: Vec<HeritageDependency<Ticket>>,
    pub initializer_origins: Vec<InitializerPoisonOrigin<Ticket>>,
    pub heritage_surface_origins: Vec<HeritageSurfacePoisonOrigin<Ticket>>,
    pub surface_origins: Vec<SurfacePoisonOrigin<Ticket>>,
    pub callables: Vec<RetainedClassCallable<Ticket>>,
}

impl<Ticket: Copy> ClassDraft<Ticket> {
    fn owned_type_parameters(&self) -> Vec<TypeParamId> {
        let mut binders: Vec<TypeParamId> = self
            .type_parameters
            .iter()
            .map(|parameter| parameter.application.id)
            .collect();
        for callable in &self.callables {
            binders.extend(callable.owned_type_parameters());
        }
        binders
    }

    fn identity_roots(&self) -> Vec<TypeId> {
        let mut roots = vec![self.instance_template, self.static_template];
        roots.extend(self.constructor_template);
        roots.extend(self.ordinary_identity_roots.iter().copied());
        roots.extend(
            self.type_parameters
                .iter()
                .filter_map(|parameter| parameter.constraint),
        );
        for parameter in &self.type_parameters {
            if let ClassTypeParameterDefault::Ready(default) = parameter.application.default {
                roots.push(default);
            }
        }
        for callable in &self.callables {
            roots.extend(callable.public_type);
            roots.extend(
                callable
                    .type_parameters
                    .iter()
                    .filter_map(|parameter| parameter.constraint),
            );
            for parameter in &callable.type_parameters {
                if let ClassTypeParameterDefault::Ready(default) = parameter.default {
                    roots.push(default);
                }
            }
            roots.extend(
                callable
                    .parameter_properties
                    .iter()
                    .map(|property| property.public_type),
            );
        }
        if let Some(heritage) = self.heritage {
            roots.push(heritage.identity_root);
        }
        roots.extend(
            self.instance_heritage
                .iter()
                .map(|heritage| heritage.identity_root),
        );
        roots
    }
}

/// Narrow class-draft coordinator. It accepts already-lowered immutable nodes
/// and preallocated owners only; no checker pass or query capability is exposed.
pub(in crate::check::checker) struct ClassSurfaceLowerer<Ticket: Copy> {
    draft: ClassDraft<Ticket>,
}

impl<Ticket: Copy> ClassSurfaceLowerer<Ticket> {
    pub(in crate::check::checker) fn new(
        class: ClassId,
        recovery_order: ClassRecoveryOrder,
        type_parameters: Vec<DraftClassTypeParameter<Ticket>>,
        instance_template: TypeId,
        static_template: TypeId,
        constructor_template: Option<TypeId>,
    ) -> Self {
        ClassSurfaceLowerer {
            draft: ClassDraft {
                class,
                recovery_order,
                type_parameters,
                instance_template,
                static_template,
                constructor_template,
                ordinary_identity_roots: Vec::new(),
                heritage: None,
                instance_heritage: Vec::new(),
                initializer_origins: Vec::new(),
                heritage_surface_origins: Vec::new(),
                surface_origins: Vec::new(),
                callables: Vec::new(),
            },
        }
    }

    #[cfg(test)]
    pub(in crate::check::checker) fn add_identity_root(&mut self, root: TypeId) {
        self.draft.ordinary_identity_roots.push(root);
    }

    pub(in crate::check::checker) fn set_heritage(
        &mut self,
        heritage: HeritageDependency<Ticket>,
    ) -> bool {
        if self.draft.heritage.is_some() {
            return false;
        }
        self.draft.heritage = Some(heritage);
        true
    }

    pub(in crate::check::checker) fn add_instance_heritage(
        &mut self,
        heritage: HeritageDependency<Ticket>,
    ) {
        self.draft.instance_heritage.push(heritage);
    }

    pub(in crate::check::checker) fn unsupported_initializer(&mut self, owner: Ticket) {
        self.draft
            .initializer_origins
            .push(InitializerPoisonOrigin { owner });
    }

    pub(in crate::check::checker) fn unsupported_heritage_surface(&mut self, owner: Ticket) {
        self.draft
            .heritage_surface_origins
            .push(HeritageSurfacePoisonOrigin { owner });
    }

    pub(in crate::check::checker) fn unsupported_surface(&mut self, owner: Ticket) {
        self.draft
            .surface_origins
            .push(SurfacePoisonOrigin { owner });
    }

    pub(in crate::check::checker) fn retain_callable(
        &mut self,
        callable: RetainedClassCallable<Ticket>,
    ) {
        self.draft.callables.push(callable);
    }

    pub(in crate::check::checker) fn finish(self) -> ClassDraft<Ticket> {
        self.draft
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ClassEdges {
    ordinary: BTreeSet<ClassId>,
    heritage: BTreeSet<ClassId>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::check::checker) struct ClassGraph {
    edges: BTreeMap<ClassId, ClassEdges>,
}

impl ClassGraph {
    pub(in crate::check::checker) fn dependencies(
        &self,
        class: ClassId,
    ) -> impl Iterator<Item = ClassId> + '_ {
        self.edges
            .get(&class)
            .into_iter()
            .flat_map(|edges| edges.ordinary.union(&edges.heritage).copied())
    }

    #[cfg(test)]
    pub(in crate::check::checker) fn ordinary_dependencies(
        &self,
        class: ClassId,
    ) -> impl Iterator<Item = ClassId> + '_ {
        self.edges
            .get(&class)
            .into_iter()
            .flat_map(|edges| edges.ordinary.iter().copied())
    }

    fn build<Ticket: Copy>(
        store: &Store,
        roots: &ReservedSurfaceRoots,
        drafts: &BTreeMap<ClassId, ClassDraft<Ticket>>,
    ) -> Result<Self, ClassPublicationError> {
        let registered: BTreeSet<ClassId> = drafts.keys().copied().collect();
        let mut graph = ClassGraph::default();
        for (&class, draft) in drafts {
            let mut edges = ClassEdges::default();
            for root in draft.identity_roots() {
                walk_identity(store, roots, root, &mut edges.ordinary);
            }
            if let Some(heritage) = draft.heritage {
                edges.heritage.insert(heritage.target);
            }
            edges.heritage.extend(
                draft
                    .instance_heritage
                    .iter()
                    .map(|heritage| heritage.target),
            );
            if let Some(target) = edges
                .ordinary
                .union(&edges.heritage)
                .find(|target| !registered.contains(target))
            {
                return Err(ClassPublicationError::UnknownClassDependency {
                    class,
                    target: *target,
                });
            }
            graph.edges.insert(class, edges);
        }
        Ok(graph)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum ClassPublicationError {
    DuplicateClass(ClassId),
    UnknownClassDependency { class: ClassId, target: ClassId },
    InvalidCallableBinder { class: ClassId },
    DuplicateOwnedBinder(TypeParamId),
    FrozenOwnedBinder(TypeParamId),
    InvalidFinalRegistry,
}

pub(in crate::check::checker) struct ClassPublication<Ticket: Copy> {
    pub published: PublishedClasses,
    #[cfg(test)]
    pub graph: ClassGraph,
    #[cfg(test)]
    pub dependency_first_sccs: Vec<Vec<ClassId>>,
    pub obligations: Vec<PendingSurfaceObligation<Ticket>>,
    /// Frozen descriptors consumed by class type-reference/new application sites.
    pub type_parameters: BTreeMap<ClassId, Vec<DraftClassTypeParameter<Ticket>>>,
    /// The exact once-lowered callable content reused by the later body walk.
    pub retained_callables: BTreeMap<ClassId, Vec<RetainedClassCallable<Ticket>>>,
    pub heritage_constructors: BTreeMap<ClassId, TypeId>,
}

pub(in crate::check::checker) struct ClassConstruction<Ticket: Copy> {
    drafts: BTreeMap<ClassId, ClassDraft<Ticket>>,
    roots: ReservedSurfaceRoots,
}

impl<Ticket: Copy> Default for ClassConstruction<Ticket> {
    fn default() -> Self {
        Self {
            drafts: BTreeMap::new(),
            roots: ReservedSurfaceRoots::default(),
        }
    }
}

impl<Ticket: Copy> ClassConstruction<Ticket> {
    pub(in crate::check::checker) fn roots_mut(&mut self) -> &mut ReservedSurfaceRoots {
        &mut self.roots
    }

    pub(in crate::check::checker) fn register(
        &mut self,
        draft: ClassDraft<Ticket>,
    ) -> Result<(), ClassPublicationError> {
        use std::collections::btree_map::Entry;
        match self.drafts.entry(draft.class) {
            Entry::Vacant(entry) => {
                entry.insert(draft);
                Ok(())
            }
            Entry::Occupied(_) => Err(ClassPublicationError::DuplicateClass(draft.class)),
        }
    }

    pub(in crate::check::checker) fn finish(
        self,
        interner: &mut Interner,
    ) -> Result<ClassPublication<Ticket>, ClassPublicationError> {
        for draft in self.drafts.values() {
            let mut previous_overload = None;
            let invalid_callable = draft.callables.iter().any(|callable| {
                if !callable.binder_alignment_is_valid() || !callable.overload_metadata_is_valid() {
                    return true;
                }
                let Some(ordinal) = callable.overload_ordinal() else {
                    return false;
                };
                let invalid_order = previous_overload.is_some_and(|previous| previous >= ordinal);
                previous_overload = Some(ordinal);
                invalid_order
            });
            if invalid_callable {
                return Err(ClassPublicationError::InvalidCallableBinder { class: draft.class });
            }
        }

        let graph = ClassGraph::build(interner.store(), &self.roots, &self.drafts)?;
        let dependency_map: BTreeMap<ClassId, BTreeSet<ClassId>> = graph
            .edges
            .keys()
            .copied()
            .map(|class| (class, graph.dependencies(class).collect()))
            .collect();
        let components = dependency_first_sccs(&dependency_map);

        let mut states = FxHashMap::default();
        let mut surfaces: FxHashMap<ClassId, PublishedClassSurface> = FxHashMap::default();
        let mut poison = FxHashMap::default();
        let mut obligations = Vec::new();
        let mut heritage_constructors = BTreeMap::new();

        for classes in &components {
            let component: BTreeSet<ClassId> = classes.iter().copied().collect();
            let mut heritage_cycle = false;
            let construction_order = heritage_construction_order(classes, &self.drafts)?;
            let recoverable_mixed_cycle =
                has_recoverable_mixed_heritage_cycle(classes, &self.drafts)?;
            for class in &construction_order {
                let Some(draft) = self.drafts.get(class) else {
                    return Err(ClassPublicationError::InvalidFinalRegistry);
                };
                if let Some(heritage) = &draft.heritage {
                    if component.contains(&heritage.target) && !recoverable_mixed_cycle {
                        heritage_cycle = true;
                        obligations.push(PendingSurfaceObligation::HeritageCycle {
                            derived: *class,
                            base: heritage.target,
                            owner: heritage.owner,
                        });
                    }
                }
            }

            let mut poisoned_base = false;
            if !heritage_cycle {
                for class in classes {
                    let Some(draft) = self.drafts.get(class) else {
                        return Err(ClassPublicationError::InvalidFinalRegistry);
                    };
                    for heritage in draft.heritage.iter().chain(&draft.instance_heritage) {
                        if states.get(&heritage.target) == Some(&ClassConstructionState::Poisoned) {
                            poisoned_base = true;
                            obligations.push(PendingSurfaceObligation::PoisonedBase {
                                derived: *class,
                                base: heritage.target,
                                owner: heritage.owner,
                            });
                        }
                    }
                }
            }

            for class in &construction_order {
                let Some(draft) = self.drafts.get(class) else {
                    return Err(ClassPublicationError::InvalidFinalRegistry);
                };
                for origin in &draft.initializer_origins {
                    obligations.push(PendingSurfaceObligation::InitializerOrigin {
                        class: *class,
                        owner: origin.owner,
                    });
                }
                for origin in &draft.heritage_surface_origins {
                    obligations.push(PendingSurfaceObligation::Deferred {
                        class: *class,
                        owner: origin.owner,
                    });
                }
                for origin in &draft.surface_origins {
                    obligations.push(PendingSurfaceObligation::SurfaceOrigin {
                        class: *class,
                        owner: origin.owner,
                    });
                }
                let cause = if heritage_cycle
                    || poisoned_base
                    || !draft.heritage_surface_origins.is_empty()
                {
                    Some(PublishedClassPoison::Heritage)
                } else if !draft.initializer_origins.is_empty() {
                    Some(PublishedClassPoison::Initializer)
                } else if !draft.surface_origins.is_empty() {
                    Some(PublishedClassPoison::Surface)
                } else {
                    None
                };
                if let Some(cause) = cause {
                    states.insert(*class, ClassConstructionState::Poisoned);
                    poison.insert(*class, cause);
                } else {
                    let (mut instance_template, static_template, inherited_constructor) =
                        if let Some(heritage) = draft.heritage {
                            match surfaces.get(&heritage.target).cloned() {
                                Some(base) => {
                                    let Some(application) = interner
                                        .store()
                                        .class_instance_type(heritage.identity_root)
                                        .cloned()
                                    else {
                                        return Err(ClassPublicationError::InvalidFinalRegistry);
                                    };
                                    let substitutions: FxHashMap<TypeParamId, TypeId> = base
                                        .type_params()
                                        .iter()
                                        .copied()
                                        .zip(application.args.iter().copied())
                                        .collect();
                                    let base_instance = substitute(
                                        interner,
                                        base.instance_template(),
                                        &substitutions,
                                    );
                                    let instance = merge_heritage_instance(
                                        interner,
                                        base_instance,
                                        draft.instance_template,
                                    )?;
                                    let base_static = substitute(
                                        interner,
                                        base.static_template(),
                                        &substitutions,
                                    );
                                    let static_side = merge_heritage_instance(
                                        interner,
                                        base_static,
                                        draft.static_template,
                                    )?;
                                    let constructor =
                                        base.constructor_template().map(|constructor| {
                                            substitute(interner, constructor, &substitutions)
                                        });
                                    (instance, static_side, constructor)
                                }
                                None if component.contains(&heritage.target) => {
                                    (draft.instance_template, draft.static_template, None)
                                }
                                None => return Err(ClassPublicationError::InvalidFinalRegistry),
                            }
                        } else {
                            (draft.instance_template, draft.static_template, None)
                        };
                    for heritage in &draft.instance_heritage {
                        let Some(base) = surfaces.get(&heritage.target).cloned() else {
                            if component.contains(&heritage.target) {
                                // Invalid interface-heritage SCCs recover in stable source
                                // order: forward edges are cut, already published back-edges
                                // retain their instance members. Static/constructor sides stay
                                // outside this soft heritage channel.
                                continue;
                            }
                            return Err(ClassPublicationError::InvalidFinalRegistry);
                        };
                        let Some(application) = interner
                            .store()
                            .class_instance_type(heritage.identity_root)
                            .cloned()
                        else {
                            return Err(ClassPublicationError::InvalidFinalRegistry);
                        };
                        let substitutions: FxHashMap<TypeParamId, TypeId> = base
                            .type_params()
                            .iter()
                            .copied()
                            .zip(application.args.iter().copied())
                            .collect();
                        let base_instance =
                            substitute(interner, base.instance_template(), &substitutions);
                        instance_template =
                            merge_heritage_instance(interner, base_instance, instance_template)?;
                    }
                    if let Some(constructor) = inherited_constructor {
                        heritage_constructors.insert(*class, constructor);
                    }
                    let constructor_template = draft
                        .constructor_template
                        .or(inherited_constructor)
                        .or_else(|| {
                            Some(interner.intern_function(FunctionType {
                                type_params: Vec::new(),
                                receiver: None,
                                params: Vec::new(),
                                ret: interner.well_known().void,
                            }))
                        });
                    states.insert(*class, ClassConstructionState::Published);
                    surfaces.insert(
                        *class,
                        PublishedClassSurface::new(
                            *class,
                            draft
                                .type_parameters
                                .iter()
                                .map(|parameter| parameter.application.id)
                                .collect(),
                            instance_template,
                            static_template,
                            constructor_template,
                        ),
                    );
                }
            }
        }

        let mut binders = Vec::new();
        for draft in self.drafts.values() {
            binders.extend(draft.owned_type_parameters());
        }
        let mut unique = FxHashSet::default();
        if let Some(id) = binders.iter().find(|id| !unique.insert(**id)) {
            return Err(ClassPublicationError::DuplicateOwnedBinder(*id));
        }
        if let Some(id) = binders
            .iter()
            .find(|id| interner.type_param_metadata_is_frozen(**id))
        {
            return Err(ClassPublicationError::FrozenOwnedBinder(*id));
        }

        let type_parameters = self
            .drafts
            .iter()
            .map(|(class, draft)| (*class, draft.type_parameters.clone()))
            .collect();
        let retained_callables = self
            .drafts
            .iter()
            .map(|(class, draft)| (*class, draft.callables.clone()))
            .collect();

        // Build the immutable registry before the only durable mutation. After a
        // successful freeze, returning this value is infallible.
        let Some(published) = PublishedClasses::from_publication(states, surfaces, poison) else {
            return Err(ClassPublicationError::InvalidFinalRegistry);
        };
        match interner.freeze_type_param_metadata(&binders) {
            Ok(()) => {}
            Err(TypeParamFreezeError::Duplicate(id)) => {
                return Err(ClassPublicationError::DuplicateOwnedBinder(id));
            }
            Err(TypeParamFreezeError::AlreadyFrozen(id)) => {
                return Err(ClassPublicationError::FrozenOwnedBinder(id));
            }
        }
        Ok(ClassPublication {
            published,
            #[cfg(test)]
            graph,
            #[cfg(test)]
            dependency_first_sccs: components,
            obligations,
            type_parameters,
            retained_callables,
            heritage_constructors,
        })
    }
}

fn merge_heritage_instance(
    interner: &mut Interner,
    base: TypeId,
    own: TypeId,
) -> Result<TypeId, ClassPublicationError> {
    let Some(mut merged) = interner.store().object_type(base).cloned() else {
        return Err(ClassPublicationError::InvalidFinalRegistry);
    };
    let Some(own) = interner.store().object_type(own).cloned() else {
        return Err(ClassPublicationError::InvalidFinalRegistry);
    };
    for property in own.properties {
        if let Some(index) = merged
            .properties
            .iter()
            .position(|inherited| inherited.name == property.name)
        {
            merged.properties[index] = property;
        } else {
            merged.properties.push(property);
        }
    }
    if own.string_index.is_some() {
        merged.string_index = own.string_index;
    }
    if own.number_index.is_some() {
        merged.number_index = own.number_index;
    }
    merged.call_signatures.extend(own.call_signatures);
    if !own.construct_signatures.is_empty() {
        // An own constructor replaces inherited construct signatures; a class with
        // no own constructor keeps the base set.
        merged.construct_signatures = own.construct_signatures;
    }
    Ok(interner.intern_object(ObjectType { ..merged }))
}

fn walk_identity(
    store: &Store,
    roots: &ReservedSurfaceRoots,
    root: TypeId,
    classes: &mut BTreeSet<ClassId>,
) {
    let mut seen = BTreeSet::new();
    let mut stack = vec![root];
    while let Some(ty) = stack.pop() {
        if !seen.insert(ty) {
            continue;
        }
        if let Some(children) = roots.children(ty) {
            stack.extend(children.iter().copied());
        }
        match store.tag(ty) {
            TypeTag::Intrinsic
            | TypeTag::Literal
            | TypeTag::TypeParam
            | TypeTag::Infer
            | TypeTag::MappedValue => {}
            TypeTag::Object => {
                if let Some(object) = store.object_type(ty) {
                    for property in &object.properties {
                        stack.push(property.ty);
                        stack.extend(property.write_ty);
                    }
                    stack.extend(object.string_index);
                    stack.extend(object.number_index);
                    stack.extend(object.call_signatures.iter().copied());
                    stack.extend(object.construct_signatures.iter().copied());
                }
            }
            TypeTag::Union => {
                if let Some(members) = store.union_members(ty) {
                    stack.extend(members.iter().copied());
                }
            }
            TypeTag::Intersection => {
                if let Some(members) = store.intersection_members(ty) {
                    stack.extend(members.iter().copied());
                }
            }
            TypeTag::Function => {
                if let Some(function) = store.function_type(ty) {
                    stack.extend(
                        function
                            .type_params
                            .iter()
                            .filter_map(|param| param.constraint),
                    );
                    stack.extend(
                        function
                            .type_params
                            .iter()
                            .filter_map(|param| param.default),
                    );
                    stack.extend(function.receiver);
                    stack.extend(function.params.iter().map(|parameter| parameter.ty));
                    stack.push(function.ret);
                }
            }
            TypeTag::Array => {
                if let Some(array) = store.array_type(ty) {
                    stack.push(array.element);
                }
            }
            TypeTag::Tuple => {
                if let Some(tuple) = store.tuple_type(ty) {
                    stack.extend(tuple.elements.iter().copied());
                    stack.extend(tuple.rest.map(|rest| rest.ty));
                }
            }
            TypeTag::Readonly => stack.extend(store.readonly_operand(ty)),
            TypeTag::Conditional => {
                if let Some(conditional) = store.conditional_type(ty) {
                    stack.extend([
                        conditional.check,
                        conditional.extends_ty,
                        conditional.true_branch,
                        conditional.false_branch,
                    ]);
                }
            }
            TypeTag::Instantiation => {
                if let Some(instantiation) = store.instantiation_type(ty) {
                    stack.push(instantiation.base);
                    stack.extend(instantiation.args.iter().map(|(_, argument)| *argument));
                }
            }
            TypeTag::Mapped => {
                if let Some(mapped) = store.mapped_type(ty) {
                    stack.push(mapped.key_source);
                    stack.push(mapped.value_template);
                    stack.extend(mapped.modifiers_source);
                }
            }
            TypeTag::Template => {
                if let Some(template) = store.template_type(ty) {
                    stack.extend(template.holes.iter().copied());
                }
            }
            TypeTag::Keyof => stack.extend(store.keyof_operand(ty)),
            TypeTag::ClassInstance => {
                if let Some(application) = store.class_instance_type(ty) {
                    classes.insert(application.class);
                    stack.extend(application.args.iter().copied());
                }
            }
            TypeTag::DeferredIndexedAccess => {
                if let Some(access) = store.deferred_indexed_access_type(ty) {
                    stack.extend([access.object, access.index]);
                }
            }
        }
    }
}

fn has_recoverable_mixed_heritage_cycle<Ticket: Copy>(
    classes: &[ClassId],
    drafts: &BTreeMap<ClassId, ClassDraft<Ticket>>,
) -> Result<bool, ClassPublicationError> {
    let members: BTreeSet<ClassId> = classes.iter().copied().collect();
    let combined: BTreeMap<ClassId, BTreeSet<ClassId>> = classes
        .iter()
        .copied()
        .map(|class| {
            let draft = drafts
                .get(&class)
                .ok_or(ClassPublicationError::InvalidFinalRegistry)?;
            let dependencies = draft
                .heritage
                .iter()
                .chain(&draft.instance_heritage)
                .map(|heritage| heritage.target)
                .filter(|target| members.contains(target))
                .collect();
            Ok((class, dependencies))
        })
        .collect::<Result<_, _>>()?;
    let components = dependency_first_sccs(&combined);
    let mut component_of = BTreeMap::new();
    for (index, component) in components.iter().enumerate() {
        for class in component {
            component_of.insert(*class, index);
        }
    }
    let mut recoverable = BTreeSet::new();
    for (index, component) in components.iter().enumerate() {
        let component_members: BTreeSet<ClassId> = component.iter().copied().collect();
        let combined_cycle = component.len() > 1
            || component.first().is_some_and(|class| {
                combined
                    .get(class)
                    .is_some_and(|dependencies| dependencies.contains(class))
            });
        if !combined_cycle {
            continue;
        }
        let mut has_hard = false;
        let mut has_soft = false;
        let hard_graph: BTreeMap<ClassId, BTreeSet<ClassId>> = component
            .iter()
            .copied()
            .map(|class| {
                let draft = drafts
                    .get(&class)
                    .ok_or(ClassPublicationError::InvalidFinalRegistry)?;
                let hard = draft
                    .heritage
                    .iter()
                    .map(|heritage| heritage.target)
                    .filter(|target| component_members.contains(target))
                    .collect::<BTreeSet<_>>();
                has_hard |= !hard.is_empty();
                has_soft |= draft
                    .instance_heritage
                    .iter()
                    .any(|heritage| component_members.contains(&heritage.target));
                Ok((class, hard))
            })
            .collect::<Result<_, _>>()?;
        let hard_cycle = dependency_first_sccs(&hard_graph)
            .into_iter()
            .any(|hard_component| {
                hard_component.len() > 1
                    || hard_component.first().is_some_and(|class| {
                        hard_graph
                            .get(class)
                            .is_some_and(|dependencies| dependencies.contains(class))
                    })
            });
        if has_hard && has_soft && !hard_cycle {
            recoverable.insert(index);
        }
    }

    let mut internal_hard_edges = 0usize;
    for class in classes {
        let draft = drafts
            .get(class)
            .ok_or(ClassPublicationError::InvalidFinalRegistry)?;
        if let Some(heritage) = &draft.heritage {
            if !members.contains(&heritage.target) {
                continue;
            }
            internal_hard_edges += 1;
            let source_component = component_of.get(class).copied();
            let target_component = component_of.get(&heritage.target).copied();
            if source_component != target_component
                || !source_component.is_some_and(|index| recoverable.contains(&index))
            {
                return Ok(false);
            }
        }
    }
    Ok(internal_hard_edges > 0)
}

fn heritage_construction_order<Ticket: Copy>(
    classes: &[ClassId],
    drafts: &BTreeMap<ClassId, ClassDraft<Ticket>>,
) -> Result<Vec<ClassId>, ClassPublicationError> {
    let members: BTreeSet<ClassId> = classes.iter().copied().collect();
    let heritage_graph: BTreeMap<ClassId, BTreeSet<ClassId>> = classes
        .iter()
        .copied()
        .map(|class| {
            let draft = drafts
                .get(&class)
                .ok_or(ClassPublicationError::InvalidFinalRegistry)?;
            let dependencies = draft
                .heritage
                .iter()
                .chain(&draft.instance_heritage)
                .map(|heritage| heritage.target)
                .filter(|target| members.contains(target))
                .collect();
            Ok((class, dependencies))
        })
        .collect::<Result<_, _>>()?;
    let heritage_components = dependency_first_sccs(&heritage_graph);
    let mut component_of = BTreeMap::new();
    let mut cyclic_components = BTreeSet::new();
    for (component_index, component) in heritage_components.iter().enumerate() {
        for class in component {
            component_of.insert(*class, component_index);
        }
        let cyclic = component.len() > 1
            || component.first().is_some_and(|class| {
                heritage_graph
                    .get(class)
                    .is_some_and(|dependencies| dependencies.contains(class))
            });
        if cyclic {
            cyclic_components.insert(component_index);
        }
    }

    let mut dependencies: BTreeMap<ClassId, BTreeSet<ClassId>> = BTreeMap::new();
    let mut dependents: BTreeMap<ClassId, BTreeSet<ClassId>> = classes
        .iter()
        .copied()
        .map(|class| (class, BTreeSet::new()))
        .collect();
    for class in classes {
        let draft = drafts
            .get(class)
            .ok_or(ClassPublicationError::InvalidFinalRegistry)?;
        let class_component = component_of.get(class).copied();
        let retained = heritage_graph
            .get(class)
            .into_iter()
            .flatten()
            .copied()
            .filter(|target| {
                let target_component = component_of.get(target).copied();
                if class_component != target_component
                    || !class_component.is_some_and(|index| cyclic_components.contains(&index))
                {
                    return true;
                }
                drafts
                    .get(target)
                    .is_some_and(|base| base.recovery_order < draft.recovery_order)
            })
            .collect::<BTreeSet<_>>();
        for target in &retained {
            dependents.entry(*target).or_default().insert(*class);
        }
        dependencies.insert(*class, retained);
    }

    let mut ready: BTreeSet<(ClassRecoveryOrder, ClassId)> = dependencies
        .iter()
        .filter(|(_, dependencies)| dependencies.is_empty())
        .map(|(class, _)| {
            (
                drafts
                    .get(class)
                    .expect("construction-order class has one draft")
                    .recovery_order,
                *class,
            )
        })
        .collect();
    let mut order = Vec::with_capacity(classes.len());
    while let Some((recovery_order, class)) = ready.iter().next().copied() {
        ready.remove(&(recovery_order, class));
        order.push(class);
        for dependent in dependents.get(&class).into_iter().flatten() {
            let Some(pending) = dependencies.get_mut(dependent) else {
                return Err(ClassPublicationError::InvalidFinalRegistry);
            };
            pending.remove(&class);
            if pending.is_empty() {
                let draft = drafts
                    .get(dependent)
                    .ok_or(ClassPublicationError::InvalidFinalRegistry)?;
                ready.insert((draft.recovery_order, *dependent));
            }
        }
    }
    if order.len() != classes.len() {
        return Err(ClassPublicationError::InvalidFinalRegistry);
    }
    Ok(order)
}

pub(in crate::check::checker) fn dependency_first_sccs<Node: Copy + Ord>(
    graph: &BTreeMap<Node, BTreeSet<Node>>,
) -> Vec<Vec<Node>> {
    fn visit<Node: Copy + Ord>(
        graph: &BTreeMap<Node, BTreeSet<Node>>,
        node: Node,
        seen: &mut BTreeSet<Node>,
        order: &mut Vec<Node>,
    ) {
        if !seen.insert(node) {
            return;
        }
        for dependency in graph.get(&node).into_iter().flatten().copied() {
            visit(graph, dependency, seen, order);
        }
        order.push(node);
    }

    fn reverse_visit<Node: Copy + Ord>(
        reverse: &BTreeMap<Node, BTreeSet<Node>>,
        node: Node,
        seen: &mut BTreeSet<Node>,
        component: &mut Vec<Node>,
    ) {
        if !seen.insert(node) {
            return;
        }
        component.push(node);
        if let Some(dependents) = reverse.get(&node) {
            for dependent in dependents {
                reverse_visit(reverse, *dependent, seen, component);
            }
        }
    }

    let mut order = Vec::new();
    let mut seen = BTreeSet::new();
    for node in graph.keys().copied() {
        visit(graph, node, &mut seen, &mut order);
    }
    let mut reverse: BTreeMap<Node, BTreeSet<Node>> =
        graph.keys().map(|node| (*node, BTreeSet::new())).collect();
    for (node, dependencies) in graph {
        for dependency in dependencies {
            reverse.entry(*dependency).or_default().insert(*node);
        }
    }
    seen.clear();
    let mut components = Vec::new();
    while let Some(node) = order.pop() {
        if seen.contains(&node) {
            continue;
        }
        let mut component = Vec::new();
        reverse_visit(&reverse, node, &mut seen, &mut component);
        component.sort_unstable();
        components.push(component);
    }

    // Kosaraju yields SCCs but its direction depends on which graph owns the first
    // pass. Stabilize the condensation explicitly: zero-outgoing dependencies first.
    let mut component_of = BTreeMap::new();
    for (index, component) in components.iter().enumerate() {
        for node in component {
            component_of.insert(*node, index);
        }
    }
    let mut dependencies: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); components.len()];
    let mut dependents: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); components.len()];
    for (&node, &component) in &component_of {
        for dependency in graph.get(&node).into_iter().flatten() {
            let Some(&target) = component_of.get(dependency) else {
                continue;
            };
            if component != target {
                dependencies[component].insert(target);
                dependents[target].insert(component);
            }
        }
    }
    let mut ready: BTreeSet<(Node, usize)> = dependencies
        .iter()
        .enumerate()
        .filter_map(|(index, dependencies)| {
            dependencies
                .is_empty()
                .then_some((components[index][0], index))
        })
        .collect();
    let mut sorted = Vec::with_capacity(components.len());
    while let Some((key, component)) = ready.iter().next().copied() {
        ready.remove(&(key, component));
        sorted.push(components[component].clone());
        for dependent in dependents[component].iter().copied() {
            dependencies[dependent].remove(&component);
            if dependencies[dependent].is_empty() {
                ready.insert((components[dependent][0], dependent));
            }
        }
    }
    sorted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class_semantics::{DemandOutcome, Exhaustion};
    use crate::source::ModuleOrdinal;
    use crate::types::repr::{ObjectType, PropertyType};

    fn draft(class: u32, template: TypeId, binder: u32) -> ClassSurfaceLowerer<u32> {
        ClassSurfaceLowerer::new(
            ClassId(class),
            ClassRecoveryOrder {
                source: SourceOrdinal::User(ModuleOrdinal::new(0)),
                binding_start: class,
                declaration_ordinal: class,
            },
            vec![DraftClassTypeParameter::source(
                TypeParamId(binder),
                None,
                None,
            )],
            template,
            template,
            None,
        )
    }

    #[test]
    fn alias_and_interface_roots_create_class_only_dependency_edges() {
        let mut interner = Interner::with_intrinsics();
        let empty = interner.intern_object(ObjectType::default());
        let target = interner.intern_class_instance(ClassId(2), vec![interner.well_known().number]);
        let alias = interner.intern_object(ObjectType::default());
        let interface = interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("marker", interner.well_known().string)],
            ..Default::default()
        });
        let mut construction = ClassConstruction::default();
        assert!(construction
            .roots_mut()
            .register(alias, ReservedRootKind::Alias, vec![target]));
        assert!(construction.roots_mut().register(
            interface,
            ReservedRootKind::Interface,
            vec![alias]
        ));
        assert_eq!(
            construction.roots.kind(alias),
            Some(ReservedRootKind::Alias)
        );
        let mut source = draft(1, empty, 1);
        source.add_identity_root(interface);
        construction.register(source.finish()).unwrap();
        construction.register(draft(2, empty, 2).finish()).unwrap();

        let publication = construction.finish(&mut interner).unwrap();
        assert!(interner.type_param_metadata_is_frozen(TypeParamId(1)));
        assert!(!interner.set_type_param_constraint(TypeParamId(1), interner.well_known().string));
        assert_eq!(
            publication
                .graph
                .ordinary_dependencies(ClassId(1))
                .collect::<Vec<_>>(),
            [ClassId(2)]
        );
        assert_eq!(
            publication.dependency_first_sccs,
            [vec![ClassId(2)], vec![ClassId(1)]]
        );
    }

    #[test]
    fn heritage_edge_inside_identity_scc_poisons_the_whole_component() {
        let mut interner = Interner::with_intrinsics();
        let empty = interner.intern_object(ObjectType::default());
        let one_to_two = interner.intern_class_instance(ClassId(2), Vec::new());
        let two_to_one = interner.intern_class_instance(ClassId(1), Vec::new());
        let mut first = draft(1, empty, 1);
        first.add_identity_root(one_to_two);
        assert!(first.set_heritage(HeritageDependency {
            target: ClassId(2),
            identity_root: one_to_two,
            owner: 10,
        }));
        let mut second = draft(2, empty, 2);
        second.add_identity_root(two_to_one);
        let mut construction = ClassConstruction::default();
        construction.register(first.finish()).unwrap();
        construction.register(second.finish()).unwrap();

        let publication = construction.finish(&mut interner).unwrap();
        assert_eq!(
            publication.dependency_first_sccs,
            [vec![ClassId(1), ClassId(2)]]
        );
        for class in [ClassId(1), ClassId(2)] {
            assert!(matches!(
                publication.published.require(class),
                DemandOutcome::Exhausted(Exhaustion::ClassHeritagePoison { .. })
            ));
        }
        assert!(publication
            .obligations
            .contains(&PendingSurfaceObligation::HeritageCycle {
                derived: ClassId(1),
                base: ClassId(2),
                owner: 10,
            }));
    }

    #[test]
    fn initializer_poison_does_not_propagate_through_ordinary_edge() {
        let mut interner = Interner::with_intrinsics();
        let empty = interner.intern_object(ObjectType::default());
        let poisoned_reference = interner.intern_class_instance(ClassId(1), Vec::new());
        let mut poisoned = draft(1, empty, 1);
        poisoned.unsupported_initializer(7);
        poisoned.unsupported_initializer(8);
        let mut ordinary = draft(2, empty, 2);
        ordinary.add_identity_root(poisoned_reference);
        let mut construction = ClassConstruction::default();
        construction.register(poisoned.finish()).unwrap();
        construction.register(ordinary.finish()).unwrap();

        let publication = construction.finish(&mut interner).unwrap();
        assert!(matches!(
            publication.published.require(ClassId(1)),
            DemandOutcome::Exhausted(Exhaustion::ClassInitializerPoison { .. })
        ));
        assert!(matches!(
            publication.published.require(ClassId(2)),
            DemandOutcome::Ready(())
        ));
        assert_eq!(
            publication
                .obligations
                .iter()
                .filter(|obligation| matches!(
                    obligation,
                    PendingSurfaceObligation::InitializerOrigin { .. }
                ))
                .count(),
            2
        );
    }

    #[test]
    fn unavailable_surface_has_a_distinct_non_replaying_poison_cause() {
        let mut interner = Interner::with_intrinsics();
        let empty = interner.intern_object(ObjectType::default());
        let mut unavailable = draft(1, empty, 1);
        unavailable.unsupported_surface(41);
        unavailable.unsupported_surface(42);
        let mut construction = ClassConstruction::default();
        construction.register(unavailable.finish()).unwrap();

        let publication = construction.finish(&mut interner).unwrap();
        assert!(matches!(
            publication.published.require(ClassId(1)),
            DemandOutcome::Exhausted(Exhaustion::ClassSurfacePoison { class: ClassId(1) })
        ));
        assert_eq!(
            publication
                .obligations
                .iter()
                .filter(|obligation| matches!(
                    obligation,
                    PendingSurfaceObligation::SurfaceOrigin {
                        class: ClassId(1),
                        ..
                    }
                ))
                .count(),
            2
        );
    }

    #[test]
    fn poisoned_base_propagates_only_through_dependency_first_heritage() {
        let mut interner = Interner::with_intrinsics();
        let empty = interner.intern_object(ObjectType::default());
        let base_application = interner.intern_class_instance(ClassId(1), Vec::new());
        let mut base = draft(1, empty, 1);
        base.unsupported_initializer(1);
        let mut derived = draft(2, empty, 2);
        assert!(derived.set_heritage(HeritageDependency {
            target: ClassId(1),
            identity_root: base_application,
            owner: 2,
        }));
        let mut construction = ClassConstruction::default();
        construction.register(derived.finish()).unwrap();
        construction.register(base.finish()).unwrap();

        let publication = construction.finish(&mut interner).unwrap();
        assert_eq!(
            publication.dependency_first_sccs,
            [vec![ClassId(1)], vec![ClassId(2)]]
        );
        assert!(matches!(
            publication.published.require(ClassId(2)),
            DemandOutcome::Exhausted(Exhaustion::ClassHeritagePoison { .. })
        ));
        assert!(publication
            .obligations
            .contains(&PendingSurfaceObligation::PoisonedBase {
                derived: ClassId(2),
                base: ClassId(1),
                owner: 2,
            }));
    }

    #[test]
    fn instance_heritage_composes_only_the_substituted_instance_surface() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let base_instance = interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("inherited", wk.number)],
            ..Default::default()
        });
        let base_static = interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("baseStatic", wk.string)],
            ..Default::default()
        });
        let base_constructor = interner.intern_function(FunctionType {
            type_params: Vec::new(),
            receiver: None,
            params: Vec::new(),
            ret: wk.number,
        });
        let own_instance = interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("own", wk.boolean)],
            ..Default::default()
        });
        let own_static = interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("ownStatic", wk.boolean)],
            ..Default::default()
        });
        let base_application = interner.intern_class_instance(ClassId(1), Vec::new());
        let base = ClassSurfaceLowerer::new(
            ClassId(1),
            ClassRecoveryOrder {
                source: SourceOrdinal::User(ModuleOrdinal::new(0)),
                binding_start: 1,
                declaration_ordinal: 1,
            },
            Vec::new(),
            base_instance,
            base_static,
            Some(base_constructor),
        );
        let mut derived = ClassSurfaceLowerer::new(
            ClassId(2),
            ClassRecoveryOrder {
                source: SourceOrdinal::User(ModuleOrdinal::new(0)),
                binding_start: 2,
                declaration_ordinal: 2,
            },
            Vec::new(),
            own_instance,
            own_static,
            None,
        );
        derived.add_instance_heritage(HeritageDependency {
            target: ClassId(1),
            identity_root: base_application,
            owner: 17,
        });
        let mut construction = ClassConstruction::default();
        construction.register(derived.finish()).unwrap();
        construction.register(base.finish()).unwrap();

        let publication = construction.finish(&mut interner).unwrap();
        assert_eq!(
            publication.dependency_first_sccs,
            [vec![ClassId(1)], vec![ClassId(2)]]
        );
        let DemandOutcome::Ready(derived) = publication.published.published_class(ClassId(2))
        else {
            panic!("instance heritage publishes its dependent class")
        };
        let instance = interner
            .store()
            .object_type(derived.instance_template())
            .expect("composed instance object");
        assert!(instance.property("inherited").is_some());
        assert!(instance.property("own").is_some());
        let static_side = interner
            .store()
            .object_type(derived.static_template())
            .expect("direct static object");
        assert!(static_side.property("ownStatic").is_some());
        assert!(static_side.property("baseStatic").is_none());
        assert_ne!(derived.constructor_template(), Some(base_constructor));
        assert!(!publication.heritage_constructors.contains_key(&ClassId(2)));
    }

    #[test]
    fn instance_heritage_cycle_recovers_by_explicit_source_order() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let first_instance = interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("ownA", wk.number)],
            ..Default::default()
        });
        let second_instance = interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("ownB", wk.string)],
            ..Default::default()
        });
        let first_static = interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("staticA", wk.number)],
            ..Default::default()
        });
        let second_static = interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("staticB", wk.string)],
            ..Default::default()
        });
        let first_application = interner.intern_class_instance(ClassId(1), Vec::new());
        let second_application = interner.intern_class_instance(ClassId(2), Vec::new());
        let mut first = ClassSurfaceLowerer::new(
            ClassId(1),
            ClassRecoveryOrder {
                source: SourceOrdinal::User(ModuleOrdinal::new(0)),
                binding_start: 10,
                declaration_ordinal: 1,
            },
            Vec::new(),
            first_instance,
            first_static,
            None,
        );
        first.add_instance_heritage(HeritageDependency {
            target: ClassId(2),
            identity_root: second_application,
            owner: 10,
        });
        let mut second = ClassSurfaceLowerer::new(
            ClassId(2),
            ClassRecoveryOrder {
                source: SourceOrdinal::User(ModuleOrdinal::new(0)),
                binding_start: 20,
                declaration_ordinal: 2,
            },
            Vec::new(),
            second_instance,
            second_static,
            None,
        );
        second.add_instance_heritage(HeritageDependency {
            target: ClassId(1),
            identity_root: first_application,
            owner: 20,
        });
        let mut construction = ClassConstruction::default();
        construction.register(second.finish()).unwrap();
        construction.register(first.finish()).unwrap();

        let publication = construction.finish(&mut interner).unwrap();
        let DemandOutcome::Ready(first) = publication.published.published_class(ClassId(1)) else {
            panic!("first recovery class is published")
        };
        let DemandOutcome::Ready(second) = publication.published.published_class(ClassId(2)) else {
            panic!("second recovery class is published")
        };
        let first_instance = interner
            .store()
            .object_type(first.instance_template())
            .expect("first recovered instance object");
        assert!(first_instance.property("ownA").is_some());
        assert!(first_instance.property("ownB").is_none());
        let second_instance = interner
            .store()
            .object_type(second.instance_template())
            .expect("second recovered instance object");
        assert!(second_instance.property("ownA").is_some());
        assert!(second_instance.property("ownB").is_some());
        let first_static = interner
            .store()
            .object_type(first.static_template())
            .expect("first recovered static object");
        assert!(first_static.property("staticB").is_none());
        let second_static = interner
            .store()
            .object_type(second.static_template())
            .expect("second recovered static object");
        assert!(second_static.property("staticA").is_none());
        assert!(publication.heritage_constructors.is_empty());
        assert!(publication.obligations.iter().all(|obligation| !matches!(
            obligation,
            PendingSurfaceObligation::HeritageCycle { .. }
                | PendingSurfaceObligation::PoisonedBase { .. }
        )));
    }

    #[test]
    fn mixed_hard_soft_cycle_recovers_without_downgrading_the_hard_edge() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let b_instance = interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("b", wk.string)],
            ..Default::default()
        });
        let a_instance = interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("a", wk.number)],
            ..Default::default()
        });
        let b_static = interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("staticB", wk.string)],
            ..Default::default()
        });
        let a_static = interner.intern_object(ObjectType {
            properties: vec![PropertyType::public("staticA", wk.number)],
            ..Default::default()
        });
        let b_application = interner.intern_class_instance(ClassId(1), Vec::new());
        let a_application = interner.intern_class_instance(ClassId(2), Vec::new());
        let mut b = ClassSurfaceLowerer::new(
            ClassId(1),
            ClassRecoveryOrder {
                source: SourceOrdinal::User(ModuleOrdinal::new(0)),
                binding_start: 10,
                declaration_ordinal: 1,
            },
            Vec::new(),
            b_instance,
            b_static,
            None,
        );
        b.add_instance_heritage(HeritageDependency {
            target: ClassId(2),
            identity_root: a_application,
            owner: 10,
        });
        let mut a = ClassSurfaceLowerer::new(
            ClassId(2),
            ClassRecoveryOrder {
                source: SourceOrdinal::User(ModuleOrdinal::new(0)),
                binding_start: 20,
                declaration_ordinal: 2,
            },
            Vec::new(),
            a_instance,
            a_static,
            None,
        );
        assert!(a.set_heritage(HeritageDependency {
            target: ClassId(1),
            identity_root: b_application,
            owner: 20,
        }));
        let mut construction = ClassConstruction::default();
        construction.register(a.finish()).unwrap();
        construction.register(b.finish()).unwrap();

        let publication = construction.finish(&mut interner).unwrap();
        let DemandOutcome::Ready(b) = publication.published.published_class(ClassId(1)) else {
            panic!("mixed-cycle B is published")
        };
        let DemandOutcome::Ready(a) = publication.published.published_class(ClassId(2)) else {
            panic!("mixed-cycle A is published")
        };
        let b_instance = interner
            .store()
            .object_type(b.instance_template())
            .expect("B instance object");
        assert!(b_instance.property("b").is_some());
        assert!(b_instance.property("a").is_none());
        let a_instance = interner
            .store()
            .object_type(a.instance_template())
            .expect("A instance object");
        assert!(a_instance.property("a").is_some());
        assert!(a_instance.property("b").is_some());
        let b_static = interner
            .store()
            .object_type(b.static_template())
            .expect("B static object");
        assert!(b_static.property("staticA").is_none());
        let a_static = interner
            .store()
            .object_type(a.static_template())
            .expect("A static object");
        assert!(a_static.property("staticB").is_some());
        assert!(!publication.heritage_constructors.contains_key(&ClassId(1)));
        assert!(publication.obligations.iter().all(|obligation| !matches!(
            obligation,
            PendingSurfaceObligation::HeritageCycle { .. }
                | PendingSurfaceObligation::PoisonedBase { .. }
        )));
    }

    #[test]
    fn pure_hard_heritage_cycle_remains_poisoned() {
        let mut interner = Interner::with_intrinsics();
        let empty = interner.intern_object(ObjectType::default());
        let first_application = interner.intern_class_instance(ClassId(1), Vec::new());
        let second_application = interner.intern_class_instance(ClassId(2), Vec::new());
        let mut first = draft(1, empty, 1);
        assert!(first.set_heritage(HeritageDependency {
            target: ClassId(2),
            identity_root: second_application,
            owner: 10,
        }));
        let mut second = draft(2, empty, 2);
        assert!(second.set_heritage(HeritageDependency {
            target: ClassId(1),
            identity_root: first_application,
            owner: 20,
        }));
        let mut construction = ClassConstruction::default();
        construction.register(second.finish()).unwrap();
        construction.register(first.finish()).unwrap();

        let publication = construction.finish(&mut interner).unwrap();
        for class in [ClassId(1), ClassId(2)] {
            assert!(matches!(
                publication.published.require(class),
                DemandOutcome::Exhausted(Exhaustion::ClassHeritagePoison { .. })
            ));
        }
        assert_eq!(
            publication
                .obligations
                .iter()
                .filter(|obligation| matches!(
                    obligation,
                    PendingSurfaceObligation::HeritageCycle { .. }
                ))
                .count(),
            2
        );
    }

    #[test]
    fn source_defaults_remain_unsupported_declaration_obligations() {
        let mut interner = Interner::with_intrinsics();
        let empty = interner.intern_object(ObjectType::default());
        let class = ClassId(1);
        let lowerer = ClassSurfaceLowerer::new(
            class,
            ClassRecoveryOrder {
                source: SourceOrdinal::User(ModuleOrdinal::new(0)),
                binding_start: 1,
                declaration_ordinal: 1,
            },
            vec![DraftClassTypeParameter::source(
                TypeParamId(4),
                None,
                Some(91),
            )],
            empty,
            empty,
            None,
        );
        let mut construction = ClassConstruction::default();
        construction.register(lowerer.finish()).unwrap();
        let publication = construction.finish(&mut interner).unwrap();
        let parameter = publication.type_parameters[&class][0].application();
        assert_eq!(parameter.id, TypeParamId(4));
        assert_eq!(
            parameter.default,
            ClassTypeParameterDefault::Unsupported(91)
        );
    }

    #[test]
    fn duplicate_binder_rejects_the_whole_freeze_batch_without_writes() {
        let mut interner = Interner::with_intrinsics();
        let empty = interner.intern_object(ObjectType::default());
        let mut construction = ClassConstruction::default();
        construction.register(draft(1, empty, 9).finish()).unwrap();
        construction.register(draft(2, empty, 9).finish()).unwrap();
        assert_eq!(
            construction.finish(&mut interner).err(),
            Some(ClassPublicationError::DuplicateOwnedBinder(TypeParamId(9)))
        );
        assert!(!interner.type_param_metadata_is_frozen(TypeParamId(9)));
        assert!(interner.set_type_param_constraint(TypeParamId(9), interner.well_known().string));
    }
}

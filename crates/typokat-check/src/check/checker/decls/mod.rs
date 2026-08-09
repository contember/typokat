//! decls module (extracted from checker/mod.rs).

use super::classes::construction::HeritageDependency;
use super::context::*;
use super::lexical_events::InterfaceOccurrenceKind;
use super::replay_index::ReplayOwner;
use super::type_groups::{
    InterfaceAlternativeKind, InterfaceTypedAlternative, PublishedTypeParameterDefault,
};
use crate::binder::declaration::{
    DeclId, DeclarationKind as BinderDeclarationKind, TypeGroupFragment, TypeGroupId,
};
use crate::binder::namespace::{MergeDisposition, SourceUnitKey};
use crate::binder::scope::ScopeId;
use crate::binder::Binder;
use crate::diagnostics::Diagnostic;
use crate::span::Span;
use crate::types::repr::{ClassId, TypeParamId, TypeTag, Visibility};
use crate::types::store::{Store, TypeId};
use crate::types::Interner;
use oxc_ast::ast::{
    Class, ClassElement, Declaration, ExportDefaultDeclarationKind, Expression, ForStatementInit,
    ForStatementLeft, Function, ObjectPropertyKind, Program, Statement, TSInterfaceDeclaration,
    TSInterfaceHeritage, TSModuleDeclaration, TSModuleDeclarationBody, TSType,
    TSTypeAliasDeclaration, TSTypeName, TSTypeParameterDeclaration, TSTypeParameterInstantiation,
};
use oxc_span::GetSpan;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::{BTreeMap, BTreeSet};

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static CLASS_ALLOCATION_EVENTS: std::cell::RefCell<Vec<ClassId>> = const {
        std::cell::RefCell::new(Vec::new())
    };
}

#[cfg(test)]
thread_local! {
    static INTERFACE_SCC_CONSTRUCTION_WORK: std::cell::RefCell<Vec<InterfaceSccConstructionWork>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct InterfaceSccConstructionWork {
    start: usize,
    end: usize,
    topology_builds: usize,
    topology_declaration_scans: usize,
    scc_builds: usize,
    scc_candidate_scans: usize,
    constructed_components: usize,
}

fn reserve_class_id(next_class_id: &mut u32) -> ClassId {
    let class = ClassId(*next_class_id);
    *next_class_id = next_class_id
        .checked_add(1)
        .expect("class identity domain exhausted");
    #[cfg(any(test, feature = "test-utils"))]
    CLASS_ALLOCATION_EVENTS.with(|events| events.borrow_mut().push(class));
    class
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn reset_class_allocation_events_for_test() {
    CLASS_ALLOCATION_EVENTS.with(|events| events.borrow_mut().clear());
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn class_allocation_events_for_test() -> Vec<ClassId> {
    CLASS_ALLOCATION_EVENTS.with(|events| events.borrow().clone())
}

#[cfg(test)]
fn reset_interface_scc_construction_work_for_test() {
    INTERFACE_SCC_CONSTRUCTION_WORK.with(|work| work.borrow_mut().clear());
}

#[cfg(test)]
fn interface_scc_construction_work_for_test() -> Vec<InterfaceSccConstructionWork> {
    INTERFACE_SCC_CONSTRUCTION_WORK.with(|work| work.borrow().clone())
}

#[cfg(test)]
mod cycle_tainted_application_cache_spec;
#[cfg(test)]
mod eager_application_cache_spec;
#[cfg(test)]
mod heritage_base_merge_scan_spec;
pub(in crate::check::checker) mod interface;
#[cfg(test)]
mod interface_scc_pending_spec;
mod params;
mod resolve;

struct InterfaceOwnMemberOwners<Ticket: Copy> {
    properties: BTreeMap<crate::types::repr::PropertyKey, (Ticket, Span)>,
    string_index: Option<(Ticket, Span)>,
    number_index: Option<(Ticket, Span)>,
}

struct ObjectAliasCanonicalizationGraph {
    local_start: usize,
    reverse: Vec<Vec<TypeId>>,
    reaches_active_reservation: Vec<bool>,
    #[cfg(test)]
    scanned_owners: usize,
}

impl ObjectAliasCanonicalizationGraph {
    fn build(store: &Store, active: &FxHashSet<TypeId>) -> Self {
        let local_start = store.base_len();
        let local_len = store.len() - local_start;
        let mut reverse = vec![Vec::new(); local_len];
        for raw_owner in local_start..store.len() {
            let owner = TypeId(u32::try_from(raw_owner).expect("type store length fits TypeId"));
            for_each_object_alias_type_operand(store, owner, |target| {
                if let Some(target) = target.index().checked_sub(local_start) {
                    reverse[target].push(owner);
                }
            });
        }

        let mut reaches_active_reservation = vec![false; local_len];
        let mut pending = active.iter().copied().collect::<Vec<_>>();
        while let Some(target) = pending.pop() {
            let target = target
                .index()
                .checked_sub(local_start)
                .expect("active object reservations belong to the local type suffix");
            if std::mem::replace(&mut reaches_active_reservation[target], true) {
                continue;
            }
            pending.extend(reverse[target].iter().copied());
        }
        Self {
            local_start,
            reverse,
            reaches_active_reservation,
            #[cfg(test)]
            scanned_owners: local_len,
        }
    }

    fn local_index(&self, ty: TypeId) -> Option<usize> {
        ty.index().checked_sub(self.local_start)
    }

    fn body_reaches_active_reservation(&self, store: &Store, root: TypeId) -> bool {
        let mut reaches = false;
        for_each_object_alias_type_operand(store, root, |target| {
            reaches |= self
                .local_index(target)
                .is_some_and(|target| self.reaches_active_reservation[target]);
        });
        reaches
    }

    fn has_external_store_inbound(&self, root: TypeId) -> bool {
        self.local_index(root)
            .and_then(|local_root| self.reverse.get(local_root))
            .expect("active object reservation belongs to the local type suffix")
            .iter()
            .any(|owner| *owner != root)
    }

    fn active_reservations_reachable_from_semantic_roots(
        &self,
        store: &Store,
        roots: impl IntoIterator<Item = TypeId>,
        active: &FxHashSet<TypeId>,
    ) -> FxHashSet<TypeId> {
        let mut reached = FxHashSet::default();
        let mut visited = vec![false; self.reverse.len()];
        let mut pending = roots.into_iter().collect::<Vec<_>>();
        while let Some(ty) = pending.pop() {
            let Some(local) = self.local_index(ty) else {
                continue;
            };
            if std::mem::replace(&mut visited[local], true) {
                continue;
            }
            if active.contains(&ty) {
                reached.insert(ty);
            }
            for_each_object_alias_type_operand(store, ty, |operand| pending.push(operand));
        }
        reached
    }
}

fn for_each_object_alias_type_operand(store: &Store, owner: TypeId, mut visit: impl FnMut(TypeId)) {
    match store.tag(owner) {
        TypeTag::Intrinsic | TypeTag::Literal | TypeTag::Infer | TypeTag::MappedValue => {}
        TypeTag::Object => {
            let object = store.object_type(owner).expect("object payload");
            for property in &object.properties {
                visit(property.ty);
                if let Some(write_ty) = property.write_ty {
                    visit(write_ty);
                }
            }
            if let Some(index) = object.string_index {
                visit(index);
            }
            if let Some(index) = object.number_index {
                visit(index);
            }
            object.call_signatures.iter().copied().for_each(&mut visit);
            object
                .construct_signatures
                .iter()
                .copied()
                .for_each(&mut visit);
        }
        TypeTag::Union => store
            .union_members(owner)
            .expect("union payload")
            .iter()
            .copied()
            .for_each(visit),
        TypeTag::Intersection => store
            .intersection_members(owner)
            .expect("intersection payload")
            .iter()
            .copied()
            .for_each(visit),
        TypeTag::Function => {
            let function = store.function_type(owner).expect("function payload");
            for parameter in &function.type_params {
                if let Some(constraint) = parameter.constraint {
                    visit(constraint);
                }
                if let Some(default) = parameter.default {
                    visit(default);
                }
            }
            if let Some(receiver) = function.receiver {
                visit(receiver);
            }
            for parameter in &function.params {
                visit(parameter.ty);
            }
            visit(function.ret);
        }
        TypeTag::TypeParam => {
            let parameter = store.type_param(owner).expect("type parameter payload");
            if let Some(constraint) = store.type_param_constraint(parameter.id) {
                visit(constraint);
            }
        }
        TypeTag::Array => visit(store.array_type(owner).expect("array payload").element),
        TypeTag::Tuple => {
            let tuple = store.tuple_type(owner).expect("tuple payload");
            tuple.elements.iter().copied().for_each(&mut visit);
            if let Some(rest) = tuple.rest {
                visit(rest.ty);
            }
        }
        TypeTag::Readonly => visit(store.readonly_operand(owner).expect("readonly payload")),
        TypeTag::Conditional => {
            let conditional = store.conditional_type(owner).expect("conditional payload");
            visit(conditional.check);
            visit(conditional.extends_ty);
            visit(conditional.true_branch);
            visit(conditional.false_branch);
        }
        TypeTag::Instantiation => {
            let instantiation = store
                .instantiation_type(owner)
                .expect("instantiation payload");
            visit(instantiation.base);
            for (_, argument) in &instantiation.args {
                visit(*argument);
            }
        }
        TypeTag::Mapped => {
            let mapped = store.mapped_type(owner).expect("mapped payload");
            visit(mapped.key_source);
            visit(mapped.value_template);
            if let Some(source) = mapped.modifiers_source {
                visit(source);
            }
        }
        TypeTag::Template => store
            .template_type(owner)
            .expect("template payload")
            .holes
            .iter()
            .copied()
            .for_each(visit),
        TypeTag::Keyof => visit(store.keyof_operand(owner).expect("keyof payload")),
        TypeTag::ClassInstance => store
            .class_instance_type(owner)
            .expect("class instance payload")
            .args
            .iter()
            .copied()
            .for_each(visit),
        TypeTag::DeferredIndexedAccess => {
            let access = store
                .deferred_indexed_access_type(owner)
                .expect("deferred indexed access payload");
            visit(access.object);
            visit(access.index);
        }
        TypeTag::Declared => {
            let declared = store.declared_type(owner).expect("declared payload");
            for (_, value) in &declared.mapper {
                visit(*value);
            }
        }
    }
}

fn push_type_parameter_descriptors(
    roots: &mut Vec<TypeId>,
    descriptors: Option<&TypeGroupParameterDescriptors>,
) {
    let Some(descriptors) = descriptors else {
        return;
    };
    roots.extend(
        descriptors
            .constraints
            .iter()
            .chain(&descriptors.defaults)
            .filter_map(|state| match state {
                TypeParameterMetadataState::Ready(ty) => Some(*ty),
                TypeParameterMetadataState::Absent
                | TypeParameterMetadataState::Poisoned
                | TypeParameterMetadataState::Unsupported => None,
            }),
    );
}

fn push_published_defaults(roots: &mut Vec<TypeId>, defaults: &[PublishedTypeParameterDefault]) {
    roots.extend(defaults.iter().filter_map(|default| match default {
        PublishedTypeParameterDefault::Ready(ty) => Some(*ty),
        PublishedTypeParameterDefault::Absent | PublishedTypeParameterDefault::Unsupported => None,
    }));
}

fn push_typed_alternatives(roots: &mut Vec<TypeId>, alternatives: &[InterfaceTypedAlternative]) {
    roots.extend(
        alternatives
            .iter()
            .flat_map(|alternative| alternative.types.iter().copied()),
    );
}

fn push_pending_effect_type_roots<Ticket: Copy>(
    roots: &mut Vec<TypeId>,
    effects: &CheckerEffects<Ticket>,
) {
    for obligation in &effects.obligations {
        match obligation {
            DeferredRelationObligation::Assign(obligation) => {
                roots.extend([obligation.src, obligation.tgt]);
            }
            DeferredRelationObligation::AssertionCompatibility(obligation) => {
                roots.extend([obligation.source, obligation.asserted]);
            }
        }
    }
    for check in &effects.constraint_checks {
        for (constraint, argument, _) in &check.checks {
            roots.extend(constraint.iter().copied());
            roots.push(*argument);
        }
        roots.extend(check.substitutions.values().copied());
    }
    for relation in &effects.interface_relations {
        roots.extend([relation.source, relation.target]);
    }
    for check in &effects.override_checks {
        roots.extend([check.own_ty, check.base_ty]);
    }
    for nested in &effects.nested {
        push_pending_effect_type_roots(roots, nested);
    }
}

impl<Ticket: Copy> Default for InterfaceOwnMemberOwners<Ticket> {
    fn default() -> Self {
        Self {
            properties: BTreeMap::new(),
            string_index: None,
            number_index: None,
        }
    }
}

#[derive(Copy, Clone)]
struct InterfaceHeritageDiagnostic<'name, Ticket: Copy> {
    owner: Ticket,
    span: Span,
    derived_name: &'name str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum InterfaceHeritagePlan {
    Complete(BTreeSet<TypeGroupId>),
    Poisoned,
    Opaque(BTreeSet<TypeGroupId>),
}

impl InterfaceHeritagePlan {
    fn terminals(&self) -> Option<&BTreeSet<TypeGroupId>> {
        match self {
            Self::Complete(terminals) | Self::Opaque(terminals) => Some(terminals),
            Self::Poisoned => None,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum IntersectionAbsorber {
    None,
    Any,
    Never,
    Unknown,
}

impl IntersectionAbsorber {
    fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::Never, _) | (_, Self::Never) => Self::Never,
            (Self::Any, _) | (_, Self::Any) => Self::Any,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::None, Self::None) => Self::None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum HeritageTypePlan {
    Complete {
        terminals: BTreeSet<TypeGroupId>,
        absorber: IntersectionAbsorber,
    },
    Poisoned,
    Opaque(BTreeSet<TypeGroupId>),
}

impl HeritageTypePlan {
    fn complete(terminals: BTreeSet<TypeGroupId>) -> Self {
        Self::Complete {
            terminals,
            absorber: IntersectionAbsorber::None,
        }
    }

    fn absorber(absorber: IntersectionAbsorber) -> Self {
        Self::Complete {
            terminals: BTreeSet::new(),
            absorber,
        }
    }

    fn into_topology_plan(self) -> InterfaceHeritagePlan {
        match self {
            Self::Complete {
                absorber: IntersectionAbsorber::Any,
                ..
            } => InterfaceHeritagePlan::Complete(BTreeSet::new()),
            Self::Complete {
                absorber: IntersectionAbsorber::Never,
                ..
            }
            | Self::Complete {
                absorber: IntersectionAbsorber::Unknown,
                ..
            } => InterfaceHeritagePlan::Opaque(BTreeSet::new()),
            Self::Opaque(terminals) => InterfaceHeritagePlan::Opaque(terminals),
            Self::Complete {
                terminals,
                absorber: IntersectionAbsorber::None,
            } => InterfaceHeritagePlan::Complete(terminals),
            Self::Poisoned => InterfaceHeritagePlan::Poisoned,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct InterfaceHeritageTopology {
    occurrences:
        rustc_hash::FxHashMap<(crate::binder::declaration::DeclId, u32), InterfaceHeritagePlan>,
}

#[derive(Clone)]
pub(in crate::check::checker) struct PreparedClassInstanceHeritage<Ticket: Copy> {
    pub(in crate::check::checker) base_name: String,
    pub(in crate::check::checker) dependency: HeritageDependency<Ticket>,
    pub(in crate::check::checker) span: Span,
}

#[derive(Clone)]
pub(in crate::check::checker) struct PreparedClassInterfaceFragment<'ast, Ticket: Copy> {
    pub(in crate::check::checker) fragment: InterfaceFragment<'ast>,
    pub(in crate::check::checker) object: crate::types::repr::ObjectType,
    pub(in crate::check::checker) method_names: BTreeSet<crate::types::repr::PropertyKey>,
    pub(in crate::check::checker) heritage_surfaces: Vec<(String, crate::types::repr::ObjectType)>,
    pub(in crate::check::checker) instance_heritage: Vec<PreparedClassInstanceHeritage<Ticket>>,
}

pub(in crate::check::checker) type PreparedClassInterfaceGroups<'ast, Ticket> =
    BTreeMap<TypeGroupId, Vec<PreparedClassInterfaceFragment<'ast, Ticket>>>;

impl InterfaceHeritageTopology {
    fn plan(
        &self,
        declaration: crate::binder::declaration::DeclId,
        heritage: &TSInterfaceHeritage<'_>,
    ) -> InterfaceHeritagePlan {
        self.occurrences
            .get(&(declaration, heritage.span.start))
            .cloned()
            .unwrap_or_else(|| InterfaceHeritagePlan::Opaque(BTreeSet::new()))
    }
}

impl<'a, 'ast, Ticket: Copy + PartialEq> Pass<'a, 'ast, Ticket> {
    fn merged_header_owner(
        &self,
        index: usize,
        declaration: crate::binder::declaration::DeclId,
        source_start: u32,
    ) -> Ticket {
        if matches!(
            self.type_decls.get(index),
            Some(TypeDecl::Class {
                declaration: class_declaration,
                ..
            }) if *class_declaration == declaration
        ) {
            return self
                .lexical_events
                .declaration_owner(declaration)
                .expect("class header has one exact declaration owner")
                .ticket;
        }
        self.lexical_events
            .interface_occurrence_owner(declaration, InterfaceOccurrenceKind::Header, source_start)
            .expect("interface header has one exact preallocated owner")
    }

    fn class_interface_header_fragments(&self, index: usize) -> Vec<InterfaceFragment<'ast>> {
        let Some(TypeDecl::Class {
            declaration,
            scope,
            class_params,
            param_decl,
            interfaces,
            ..
        }) = self.type_decls.get(index)
        else {
            return Vec::new();
        };
        let mut fragments = interfaces.clone();
        fragments.push(InterfaceFragment {
            declaration: *declaration,
            scope: *scope,
            param_decl: *param_decl,
            params: class_params.clone(),
            members: &[],
            extends: &[],
        });
        let group = TypeGroupId(u32::try_from(index).expect("type group index fits u32"));
        sort_interface_fragments(self.binder, group, &mut fragments);
        fragments
    }

    fn lower_type_group_parameter_metadata(&mut self, index: usize) {
        let class_fragments = self.class_interface_header_fragments(index);
        let (scope, param_decl, params, header_declaration) = match self.type_decls.get(index) {
            Some(TypeDecl::Interface {
                declaration,
                scope,
                param_decl,
                params,
                ..
            }) => (*scope, *param_decl, params.clone(), Some(*declaration)),
            Some(TypeDecl::Class { interfaces, .. }) if !interfaces.is_empty() => {
                let canonical = class_fragments
                    .first()
                    .expect("merged class group has one canonical header");
                (
                    canonical.scope,
                    canonical.param_decl,
                    canonical.params.clone(),
                    Some(canonical.declaration),
                )
            }
            Some(TypeDecl::Alias {
                scope,
                param_decl,
                params,
                ..
            }) => (*scope, *param_decl, params.clone(), None),
            _ => return,
        };
        let group = TypeGroupId(u32::try_from(index).expect("type group index fits u32"));
        let metadata_params = params.clone();
        let frame = self.build_type_param_frame(param_decl, &params);
        let validate_locally = header_declaration.is_none();
        let lower = |pass: &mut Self| {
            pass.with_type_params(frame, |pass| {
                pass.lower_type_group_parameter_descriptors(
                    scope,
                    param_decl,
                    &params,
                    validate_locally,
                )
            })
        };
        let descriptors = if let Some(declaration) = header_declaration {
            let header_span = self
                .binder
                .declarations
                .get(declaration)
                .map(|declaration| declaration.site.binding_span)
                .unwrap_or(Span::new(0, 0));
            let owner = self.merged_header_owner(index, declaration, header_span.start);
            self.with_exact_type_declaration_source(declaration, |pass| {
                pass.with_replay_owner(super::replay_index::ReplayOwner::TypeGroup(group), |pass| {
                    pass.with_ticket_effects(owner, lower)
                })
            })
        } else {
            self.with_type_decl_effects(group, lower)
        };
        let lowered_defaults = descriptors
            .defaults
            .iter()
            .copied()
            .map(TypeParameterMetadataState::ready)
            .collect::<Vec<_>>();
        match self.type_decls.get_mut(index) {
            Some(TypeDecl::Interface {
                params,
                recovery_params,
                recovery_defaults,
                defaults: target,
                parameter_descriptors,
                ..
            }) => {
                *target = lowered_defaults;
                let parameter_defaults =
                    descriptors
                        .defaults
                        .iter()
                        .copied()
                        .map(|default| match default {
                            TypeParameterMetadataState::Absent => {
                                PublishedTypeParameterDefault::Absent
                            }
                            TypeParameterMetadataState::Ready(default) => {
                                PublishedTypeParameterDefault::Ready(default)
                            }
                            TypeParameterMetadataState::Poisoned
                            | TypeParameterMetadataState::Unsupported => {
                                PublishedTypeParameterDefault::Unsupported
                            }
                        });
                for (&parameter, default) in params.iter().zip(parameter_defaults) {
                    let recovery_index = recovery_params
                        .iter()
                        .position(|candidate| *candidate == parameter)
                        .expect("canonical interface parameter is a recovery parameter");
                    if recovery_defaults[recovery_index] == PublishedTypeParameterDefault::Absent {
                        recovery_defaults[recovery_index] = default;
                    }
                }
                *parameter_descriptors = Some(descriptors);
            }
            Some(TypeDecl::Class {
                params: recovery_params,
                recovery_defaults,
                parameter_descriptors,
                ..
            }) => {
                let parameter_defaults =
                    descriptors
                        .defaults
                        .iter()
                        .copied()
                        .map(|default| match default {
                            TypeParameterMetadataState::Absent => {
                                PublishedTypeParameterDefault::Absent
                            }
                            TypeParameterMetadataState::Ready(default) => {
                                PublishedTypeParameterDefault::Ready(default)
                            }
                            TypeParameterMetadataState::Poisoned
                            | TypeParameterMetadataState::Unsupported => {
                                PublishedTypeParameterDefault::Unsupported
                            }
                        });
                for (&parameter, default) in metadata_params.iter().zip(parameter_defaults) {
                    let recovery_index = recovery_params
                        .iter()
                        .position(|candidate| *candidate == parameter)
                        .expect("canonical class parameter is a recovery parameter");
                    if recovery_defaults[recovery_index] == PublishedTypeParameterDefault::Absent {
                        recovery_defaults[recovery_index] = default;
                    }
                }
                *parameter_descriptors = Some(descriptors);
            }
            Some(TypeDecl::Alias {
                defaults: target, ..
            }) => *target = lowered_defaults,
            _ => unreachable!("type-group parameter owner changed during lowering"),
        }
    }

    fn with_type_decl_effects<R>(
        &mut self,
        decl_id: TypeGroupId,
        produce: impl FnOnce(&mut Self) -> R,
    ) -> R {
        if self.current_replay_owner().is_some() {
            self.record_replay_demand(super::replay_index::ReplayOwner::TypeGroup(decl_id));
        }
        let declaration = match self.type_decls.get(decl_id.index()) {
            Some(TypeDecl::Interface { declaration, .. })
            | Some(TypeDecl::Alias { declaration, .. })
            | Some(TypeDecl::Class { declaration, .. })
            | Some(TypeDecl::Unavailable { declaration }) => *declaration,
            Some(TypeDecl::Resolved { .. }) | None => {
                panic!("published prelude groups must not re-enter private construction")
            }
        };
        let owner = self
            .lexical_events
            .declaration_owner(declaration)
            .expect("type declaration must have a preallocated lexical owner");
        self.with_type_declaration_source(declaration, |pass| {
            pass.with_replay_owner(
                super::replay_index::ReplayOwner::TypeGroup(decl_id),
                |pass| pass.with_ticket_effects(owner.ticket, produce),
            )
        })
    }

    /// Nested endpoints retain the outer type declaration's provenance.
    fn with_type_declaration_source<R>(
        &mut self,
        declaration: DeclId,
        produce: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let source = self
            .lexical_events
            .declaration_source(declaration)
            .map(|source| source.unit);
        let previous_source = self.current_source;
        let previous_root_source = self.early_native_array_root_source;
        if let Some(source) = source {
            self.current_source = source;
            if previous_root_source.is_none() {
                self.early_native_array_root_source = Some(source);
            }
        } else {
            self.early_native_array_root_source = None;
        }
        let result = produce(self);
        self.current_source = previous_source;
        self.early_native_array_root_source = previous_root_source;
        result
    }

    /// A merged interface fragment overrides its group's provenance.
    fn with_exact_type_declaration_source<R>(
        &mut self,
        declaration: DeclId,
        produce: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let source = self
            .lexical_events
            .declaration_source(declaration)
            .map(|source| source.unit);
        let previous_source = self.current_source;
        let previous_root_source = self.early_native_array_root_source;
        if let Some(source) = source {
            self.current_source = source;
            self.early_native_array_root_source = Some(source);
        } else {
            self.early_native_array_root_source = None;
        }
        let result = produce(self);
        self.current_source = previous_source;
        self.early_native_array_root_source = previous_root_source;
        result
    }

    pub(super) fn resolve_type_decl(&mut self, scope: ScopeId, decl_id: TypeGroupId) -> TypeId {
        if self
            .type_resolved
            .get(decl_id.index())
            .copied()
            .flatten()
            .is_some()
            && !self.type_group_construction_is_pending(decl_id)
        {
            return self.resolve_type_decl_inner(scope, decl_id);
        }
        self.with_type_decl_effects(decl_id, |pass| pass.resolve_type_decl_inner(scope, decl_id))
    }

    pub(super) fn emit_type_decl_diagnostic(
        &mut self,
        decl_id: TypeGroupId,
        diagnostic: crate::diagnostics::Diagnostic,
    ) {
        self.with_type_decl_effects(decl_id, |pass| pass.emit_diagnostic(diagnostic));
    }

    /// Fill named type declarations so later annotation reads are plain id lookups.
    ///
    /// Interfaces fill before aliases because alias instantiation must substitute over
    /// an already-filled generic interface template; reverse dependencies stay lazy.
    pub(in crate::check::checker) fn fill_type_decls(&mut self, scope: ScopeId) {
        self.fill_type_decls_range(
            scope,
            self.type_decls.published_len(),
            self.type_decls.len(),
        );
    }

    pub(in crate::check::checker) fn fill_type_decls_range(
        &mut self,
        scope: ScopeId,
        start: usize,
        end: usize,
    ) {
        // Template lowering keeps conditionals lazy until value-position demand.
        let building_template = std::mem::replace(&mut self.building_template, true);

        for index in start..end {
            self.lower_type_group_parameter_metadata(index);
        }

        if start == self.type_decls.published_len() {
            let replacement_indices = self.type_decls.replacement_indices();
            let replacement_interfaces = replacement_indices
                .iter()
                .copied()
                .filter(|index| {
                    matches!(
                        self.type_decls.get(*index),
                        Some(TypeDecl::Interface { .. })
                    )
                })
                .collect::<Vec<_>>();
            self.building_template = true;
            for index in replacement_interfaces.iter().copied() {
                self.lower_type_group_parameter_metadata(index);
            }
            let mut interface_candidates = replacement_interfaces.clone();
            interface_candidates.extend((start..end).filter(|index| {
                matches!(
                    self.type_decls.get(*index),
                    Some(TypeDecl::Interface { .. })
                )
            }));
            if !interface_candidates.is_empty() {
                self.construct_pending_interface_candidates(&interface_candidates, start, end);
            }
            for index in replacement_indices {
                if replacement_interfaces.binary_search(&index).is_err() {
                    self.fill_type_decls_range(scope, index, index + 1);
                }
            }
        } else {
            // Freeze interface dependency components before aliases can observe them.
            self.construct_pending_interface_sccs(start, end);
        }

        // Fill conditional-alias placeholders before ordinary aliases can instantiate them.
        for index in start..end {
            let (scope, placeholder, params, param_decl, annotation, name, name_span) =
                match &self.type_decls[index] {
                    TypeDecl::Alias {
                        scope,
                        conditional_template: Some(placeholder),
                        params,
                        param_decl,
                        annotation,
                        name,
                        name_span,
                        ..
                    } => (
                        *scope,
                        *placeholder,
                        params.clone(),
                        *param_decl,
                        *annotation,
                        name.clone(),
                        *name_span,
                    ),
                    _ => continue,
                };
            let decl_id = TypeGroupId(u32::try_from(index).expect("type group index fits u32"));
            self.begin_type_group_construction(decl_id);
            let frame = self.build_type_param_frame(param_decl, &params);
            self.resolving_conditional_alias = Some((decl_id, name_span, name));
            let lowered = self.with_type_decl_effects(decl_id, |pass| {
                pass.with_type_params(frame, |pass| pass.lower_annotation(scope, annotation))
            });
            self.resolving_conditional_alias = None;

            let error_ty = self.interner.well_known().error;
            match lowered {
                Some(id) if self.interner.store().tag(id) == TypeTag::Conditional => {
                    // Copy the freshly-lowered conditional's body into the reserved
                    // template id, so self-recursive instantiations point at a filled node.
                    if let Some(cond) = self.interner.store().conditional_type(id).copied() {
                        self.interner.fill_conditional(placeholder, cond);
                    }
                }
                // Circular check (`TK2456`) or out-of-subset body → the alias is the error
                // type (silent downstream, m22 discipline).
                _ => {
                    self.interner
                        .poison_reserved_conditional(placeholder)
                        .expect("failed conditional alias owns one pending reservation");
                    if let Some(slot) = self.type_resolved.get_mut(index) {
                        *slot = Some(error_ty);
                    }
                    if let TypeDecl::Alias {
                        conditional_template,
                        ..
                    } = &mut self.type_decls[index]
                    {
                        *conditional_template = None;
                    }
                }
            }
            self.freeze_type_group(decl_id);
        }

        // Fill mapped-alias placeholders before ordinary aliases can instantiate them.
        for index in start..end {
            let (scope, placeholder, params, param_decl, annotation, name, name_span) =
                match &self.type_decls[index] {
                    TypeDecl::Alias {
                        scope,
                        mapped_template: Some(placeholder),
                        params,
                        param_decl,
                        annotation,
                        name,
                        name_span,
                        ..
                    } => (
                        *scope,
                        *placeholder,
                        params.clone(),
                        *param_decl,
                        *annotation,
                        name.clone(),
                        *name_span,
                    ),
                    _ => continue,
                };
            let decl_id = TypeGroupId(u32::try_from(index).expect("type group index fits u32"));
            self.begin_type_group_construction(decl_id);
            let frame = self.build_type_param_frame(param_decl, &params);
            let prev_resolving_alias = self.resolving_alias.take();
            self.resolving_alias = Some((decl_id, name_span, name.clone()));
            self.resolving_alias_stack.push((
                decl_id,
                name_span,
                name,
                self.alias_indirection_depth,
            ));
            let lowered = self.with_type_decl_effects(decl_id, |pass| {
                pass.with_type_params(frame, |pass| pass.lower_annotation(scope, annotation))
            });
            self.resolving_alias_stack.pop();
            self.resolving_alias = prev_resolving_alias;

            let error_ty = self.interner.well_known().error;
            match lowered {
                Some(id) if self.interner.store().tag(id) == TypeTag::Mapped => {
                    // Copy the freshly-lowered mapped body into the reserved template
                    // id, so self-recursive instantiations point at a filled node.
                    if let Some(mapped) = self.interner.store().mapped_type(id).copied() {
                        self.interner.fill_mapped(placeholder, mapped);
                    }
                }
                // Circular key source (`TK2456`) or out-of-subset body → the alias is
                // the error type (silent downstream, M22 discipline; overwrites the
                // seeded reserved id).
                _ => {
                    self.interner
                        .poison_reserved_mapped(placeholder)
                        .expect("failed mapped alias owns one pending reservation");
                    if let Some(slot) = self.type_resolved.get_mut(index) {
                        *slot = Some(error_ty);
                    }
                    if let TypeDecl::Alias {
                        mapped_template, ..
                    } = &mut self.type_decls[index]
                    {
                        *mapped_template = None;
                    }
                }
            }
            self.freeze_type_group(decl_id);
        }

        // Fill every seeded object alias before deciding which roots are acyclic and structural.
        self.fill_and_canonicalize_object_aliases_range(start, end);

        // Touch remaining aliases to resolve the whole memoized DAG.
        for index in start..end {
            if matches!(self.type_decls[index], TypeDecl::Alias { .. }) {
                self.resolve_type_decl(
                    scope,
                    TypeGroupId(u32::try_from(index).expect("type declaration index fits u32")),
                );
            }
        }

        // Value-position annotations may evaluate conditionals after fill.
        self.building_template = building_template;
    }

    pub(in crate::check::checker) fn fill_pending_interfaces_range(
        &mut self,
        scope: ScopeId,
        start: usize,
        end: usize,
    ) {
        let building_template = std::mem::replace(&mut self.building_template, true);
        let _ = scope;
        self.construct_pending_interface_sccs(start, end);
        self.building_template = building_template;
    }

    fn collect_class_interface_heritage_type(
        &self,
        ty: TypeId,
        objects: &mut Vec<crate::types::repr::ObjectType>,
        classes: &mut Vec<PreparedClassInstanceHeritage<Ticket>>,
        owner: Ticket,
        base_name: &str,
        span: Span,
    ) -> bool {
        if let Some(object) = self.interner.store().object_type(ty).cloned() {
            objects.push(object);
            return true;
        }
        if let Some(members) = self.interner.store().intersection_members(ty) {
            return members.iter().copied().all(|member| {
                self.collect_class_interface_heritage_type(
                    member, objects, classes, owner, base_name, span,
                )
            });
        }
        if let Some(application) = self.interner.store().class_instance_type(ty) {
            classes.push(PreparedClassInstanceHeritage {
                base_name: base_name.to_string(),
                dependency: HeritageDependency {
                    target: application.class,
                    identity_root: ty,
                    owner,
                },
                span,
            });
            return true;
        }
        let well_known = self.interner.well_known();
        ty == well_known.any || ty == well_known.unknown || ty == well_known.error
    }

    fn report_cyclic_class_interface_heritage(&mut self, topology: &InterfaceHeritageTopology) {
        for component in class_interface_heritage_sccs(self.binder, &self.type_decls, topology) {
            if !class_interface_component_has_cycle(&self.type_decls, &component, topology)
                || !class_interface_component_has_soft_edge(&self.type_decls, &component, topology)
            {
                continue;
            }
            let component_set: BTreeSet<usize> = component.iter().copied().collect();
            for index in component {
                let Some(TypeDecl::Class {
                    declaration,
                    recovery_names,
                    interfaces,
                    ..
                }) = self.type_decls.get(index)
                else {
                    continue;
                };
                let declaration = *declaration;
                let recovery_names = recovery_names.clone();
                let interfaces = interfaces.clone();
                let name = self
                    .binder
                    .type_groups
                    .get(TypeGroupId(
                        u32::try_from(index).expect("type group index fits u32"),
                    ))
                    .map(|group| group.name.clone())
                    .unwrap_or_else(|| "<class>".to_string());
                let class_display = if recovery_names.is_empty() {
                    name.clone()
                } else {
                    format!("{}<{}>", name, recovery_names.join(", "))
                };
                let class_span = self
                    .binder
                    .declarations
                    .get(declaration)
                    .map(|declaration| declaration.site.binding_span)
                    .unwrap_or(Span::new(0, 0));
                let class_owner = self.merged_header_owner(index, declaration, class_span.start);
                self.with_ticket_effects(class_owner, |pass| {
                    pass.emit_diagnostic(Diagnostic::circular_interface_heritage(
                        class_span,
                        &class_display,
                    ));
                });

                for fragment in interfaces {
                    let participates = fragment.extends.iter().any(|heritage| {
                        topology
                            .plan(fragment.declaration, heritage)
                            .terminals()
                            .is_some_and(|terminals| {
                                terminals
                                    .iter()
                                    .any(|group| component_set.contains(&group.index()))
                            })
                    });
                    if !participates {
                        continue;
                    }
                    let parameter_names = fragment
                        .param_decl
                        .iter()
                        .flat_map(|declaration| declaration.params.iter())
                        .map(|parameter| parameter.name.name.as_str())
                        .collect::<Vec<_>>();
                    let display = if parameter_names.is_empty() {
                        name.clone()
                    } else {
                        format!("{}<{}>", name, parameter_names.join(", "))
                    };
                    let span = self
                        .binder
                        .declarations
                        .get(fragment.declaration)
                        .map(|declaration| declaration.site.binding_span)
                        .unwrap_or(Span::new(0, 0));
                    let owner = self.merged_header_owner(index, fragment.declaration, span.start);
                    self.with_ticket_effects(owner, |pass| {
                        pass.emit_diagnostic(Diagnostic::circular_interface_heritage(
                            span, &display,
                        ));
                    });
                }
            }
        }
    }

    /// Lower class-owned interface fragments while every type surface is still private.
    /// The returned objects are merged into their class instance before publication.
    pub(in crate::check::checker) fn prepare_class_interface_groups(
        &mut self,
    ) -> PreparedClassInterfaceGroups<'ast, Ticket> {
        let groups = self
            .type_decls
            .changed_entries()
            .into_iter()
            .filter_map(|(index, declaration)| match declaration {
                TypeDecl::Class { interfaces, .. } if !interfaces.is_empty() => Some(index),
                _ => None,
            })
            .collect::<Vec<_>>();
        let topology = interface_heritage_topology(self.binder, &self.type_decls);
        self.report_cyclic_class_interface_heritage(&topology);
        let building_template = std::mem::replace(&mut self.building_template, true);
        let mut prepared = BTreeMap::new();
        for index in groups {
            let group = TypeGroupId(u32::try_from(index).expect("type group index fits u32"));
            let _replay_owner_scope = self
                .replay_trace
                .as_ref()
                .map(|trace| trace.scope(super::replay_index::ReplayOwner::TypeGroup(group)));
            let headers = self.class_interface_header_fragments(index);
            self.validate_interface_group_headers(index, &headers);
            let interfaces = match &self.type_decls[index] {
                TypeDecl::Class { interfaces, .. } => interfaces.clone(),
                _ => unreachable!("prepared group remains class-owned"),
            };
            let mut lowered = Vec::with_capacity(interfaces.len());
            let mut conflict_inputs = Vec::with_capacity(interfaces.len());
            for fragment in interfaces {
                let frame = self.build_type_param_frame(fragment.param_decl, &fragment.params);
                let own = self.with_exact_type_declaration_source(fragment.declaration, |pass| {
                    pass.with_type_params(frame.clone(), |pass| {
                        pass.lower_interface_declaration_members(
                            fragment.declaration,
                            fragment.scope,
                            fragment.members,
                        )
                    })
                });
                let method_names = own.method_keys;
                let own = own.object;
                conflict_inputs.push((fragment.clone(), own.clone()));

                let mut heritage_surfaces = Vec::new();
                let mut instance_heritage = Vec::new();
                for heritage in fragment.extends {
                    let heritage_span = Span::from_oxc(heritage.span);
                    let owner = self
                        .lexical_events
                        .interface_occurrence_owner(
                            fragment.declaration,
                            InterfaceOccurrenceKind::Heritage,
                            heritage_span.start,
                        )
                        .expect("interface heritage has one exact preallocated owner");
                    let plan = topology.plan(fragment.declaration, heritage);
                    let resolved =
                        self.with_exact_type_declaration_source(fragment.declaration, |pass| {
                            pass.with_ticket_effects(owner, |pass| {
                                pass.with_type_params(frame.clone(), |pass| match plan {
                                    InterfaceHeritagePlan::Complete(_) => {
                                        pass.ensure_heritage_base_filled(fragment.scope, heritage);
                                        pass.resolve_heritage_type(fragment.scope, heritage)
                                    }
                                    InterfaceHeritagePlan::Poisoned => {
                                        pass.diagnose_poisoned_interface_heritage(
                                            fragment.scope,
                                            heritage,
                                        );
                                        None
                                    }
                                    InterfaceHeritagePlan::Opaque(_) => {
                                        pass.record_opaque_interface_heritage(
                                            fragment.scope,
                                            heritage,
                                        );
                                        None
                                    }
                                })
                            })
                        });
                    if let Some(resolved) = resolved {
                        let base_name = heritage_display_name(heritage);
                        let mut objects = Vec::new();
                        let mut classes = Vec::new();
                        if self.collect_class_interface_heritage_type(
                            resolved,
                            &mut objects,
                            &mut classes,
                            owner,
                            &base_name,
                            heritage_span,
                        ) {
                            if !objects.is_empty() {
                                let object =
                                    interface::merge_intersection_objects(self.interner, objects);
                                heritage_surfaces.push((base_name, object));
                            }
                            instance_heritage.extend(classes);
                        } else {
                            self.with_ticket_effects(owner, |pass| {
                                pass.record_opaque_interface_heritage(fragment.scope, heritage);
                            });
                        }
                    }
                }
                lowered.push(PreparedClassInterfaceFragment {
                    fragment,
                    object: own,
                    method_names,
                    heritage_surfaces,
                    instance_heritage,
                });
            }
            let alternatives = self.validate_interface_fragment_conflicts(&conflict_inputs);
            let TypeDecl::Class {
                conflict_alternatives,
                ..
            } = &mut self.type_decls[index]
            else {
                unreachable!()
            };
            *conflict_alternatives = alternatives;
            prepared.insert(
                TypeGroupId(u32::try_from(index).expect("type group index fits u32")),
                lowered,
            );
        }
        self.building_template = building_template;
        prepared
    }

    pub(in crate::check::checker) fn validate_class_interface_member_conflicts(
        &mut self,
        group: TypeGroupId,
        class_object: &crate::types::repr::ObjectType,
        heritage_own: &crate::types::repr::ObjectType,
        interfaces: &[PreparedClassInterfaceFragment<'ast, Ticket>],
    ) {
        #[derive(Copy, Clone, PartialEq, Eq)]
        enum DeclarationKind {
            Property,
            Method,
            Getter,
            Setter,
        }
        #[derive(Copy, Clone, PartialEq, Eq)]
        enum DeclarationOrigin {
            Class,
            Interface,
        }
        #[derive(Clone)]
        struct Member<Ticket: Copy> {
            key: crate::types::repr::PropertyKey,
            name: String,
            declaration_kind: DeclarationKind,
            declaration_origin: DeclarationOrigin,
            ty: TypeId,
            write_ty: Option<TypeId>,
            accessor_is_paired: bool,
            optional: bool,
            readonly: bool,
            visibility: Visibility,
            owner: Ticket,
            span: Span,
            order: (SourceUnitKey, u32, u32),
        }

        impl<Ticket: Copy> Member<Ticket> {
            fn is_property_declaration(&self) -> bool {
                self.declaration_kind != DeclarationKind::Method
            }

            fn conflicts_as_later_duplicate(&self, first: &Self) -> bool {
                self.declaration_kind == DeclarationKind::Method
                    || (self.declaration_kind == DeclarationKind::Setter
                        && !self.accessor_is_paired)
                    || (first.declaration_origin == DeclarationOrigin::Interface
                        && first.declaration_kind == DeclarationKind::Property
                        && self.accessor_is_paired
                        && matches!(
                            self.declaration_kind,
                            DeclarationKind::Getter | DeclarationKind::Setter
                        ))
            }

            fn comparison_ty(&self) -> TypeId {
                if self.declaration_kind == DeclarationKind::Setter && !self.accessor_is_paired {
                    self.write_ty.unwrap_or(self.ty)
                } else {
                    self.ty
                }
            }
        }

        let Some(TypeDecl::Class {
            declaration,
            class_id,
            class,
            ..
        }) = self.type_decls.get(group.index())
        else {
            return;
        };
        let class_id = *class_id;
        let declaration = *declaration;
        let class = *class;
        let group_fragments = self
            .binder
            .type_groups
            .get(group)
            .map(|group| group.fragments.as_slice())
            .unwrap_or_default();
        let fragment_source = |declaration| {
            group_fragments
                .iter()
                .find(|fragment: &&TypeGroupFragment| fragment.declaration == declaration)
                .map_or(SourceUnitKey::SINGLE_SOURCE, |fragment| fragment.source)
        };
        let class_source = fragment_source(declaration);
        let Some(reservation) = self.lexical_events.classes().iter().find(|reservation| {
            reservation
                .binding
                .as_ref()
                .is_some_and(|binding| binding.class_id == class_id)
        }) else {
            return;
        };
        let mut accessor_kinds = BTreeMap::new();
        for element in &class.body.body {
            let oxc_ast::ast::ClassElement::MethodDefinition(method) = element else {
                continue;
            };
            if method.r#static || method.computed {
                continue;
            }
            let Some(name) = method.key.static_name().map(|name| name.into_owned()) else {
                continue;
            };
            let kinds = accessor_kinds.entry(name).or_insert((false, false));
            match method.kind {
                oxc_ast::ast::MethodDefinitionKind::Get => kinds.0 = true,
                oxc_ast::ast::MethodDefinitionKind::Set => kinds.1 = true,
                oxc_ast::ast::MethodDefinitionKind::Method
                | oxc_ast::ast::MethodDefinitionKind::Constructor => {}
            }
        }
        let paired_accessors = accessor_kinds
            .into_iter()
            .filter_map(|(name, (getter, setter))| (getter && setter).then_some(name))
            .collect::<BTreeSet<_>>();
        let mut members = Vec::new();
        for (index, element) in class.body.body.iter().enumerate() {
            let owner = reservation
                .members
                .get(index)
                .and_then(|member| self.lexical_events.member(*member))
                .map_or(reservation.tickets.immediate, |member| {
                    member.tickets.immediate
                });
            match element {
                oxc_ast::ast::ClassElement::PropertyDefinition(property)
                    if !property.r#static && !property.computed =>
                {
                    let Some(name) = property.key.static_name().map(|name| name.into_owned())
                    else {
                        continue;
                    };
                    let Some(surface) = class_object.property(&name) else {
                        continue;
                    };
                    members.push(Member {
                        key: crate::types::repr::PropertyKey::String(name.clone()),
                        name,
                        declaration_kind: DeclarationKind::Property,
                        declaration_origin: DeclarationOrigin::Class,
                        ty: surface.ty,
                        write_ty: surface.write_ty,
                        accessor_is_paired: false,
                        optional: property.optional,
                        readonly: property.readonly,
                        visibility: surface.visibility,
                        owner,
                        span: Span::from_oxc(property.span),
                        order: (class_source, property.span.start, declaration.0),
                    });
                }
                oxc_ast::ast::ClassElement::MethodDefinition(method)
                    if !method.r#static
                        && !method.computed
                        && matches!(
                            method.kind,
                            oxc_ast::ast::MethodDefinitionKind::Method
                                | oxc_ast::ast::MethodDefinitionKind::Get
                                | oxc_ast::ast::MethodDefinitionKind::Set
                        ) =>
                {
                    let Some(name) = method.key.static_name().map(|name| name.into_owned()) else {
                        continue;
                    };
                    let Some(surface) = class_object.property(&name) else {
                        continue;
                    };
                    let accessor_is_paired = paired_accessors.contains(&name);
                    members.push(Member {
                        key: crate::types::repr::PropertyKey::String(name.clone()),
                        name,
                        declaration_kind: match method.kind {
                            oxc_ast::ast::MethodDefinitionKind::Method => DeclarationKind::Method,
                            oxc_ast::ast::MethodDefinitionKind::Get => DeclarationKind::Getter,
                            oxc_ast::ast::MethodDefinitionKind::Set => DeclarationKind::Setter,
                            oxc_ast::ast::MethodDefinitionKind::Constructor => unreachable!(),
                        },
                        declaration_origin: DeclarationOrigin::Class,
                        ty: surface.ty,
                        write_ty: surface.write_ty,
                        accessor_is_paired,
                        optional: false,
                        // Accessors and interface properties merge as property declarations;
                        // accessor mutability does not constitute a TS declaration modifier.
                        readonly: false,
                        visibility: surface.visibility,
                        owner,
                        span: Span::from_oxc(method.span),
                        order: (class_source, method.span.start, declaration.0),
                    });
                }
                _ => {}
            }
        }
        for prepared in interfaces {
            for signature in prepared.fragment.members {
                let (key, declaration_kind, optional, readonly, span) = match signature {
                    oxc_ast::ast::TSSignature::TSPropertySignature(signature) => {
                        let Some(key) = self.signature_property_key(
                            prepared.fragment.scope,
                            &signature.key,
                            signature.computed,
                        ) else {
                            continue;
                        };
                        (
                            key,
                            DeclarationKind::Property,
                            signature.optional,
                            signature.readonly,
                            Span::from_oxc(signature.span),
                        )
                    }
                    oxc_ast::ast::TSSignature::TSMethodSignature(signature) => {
                        let Some(key) = self.signature_property_key(
                            prepared.fragment.scope,
                            &signature.key,
                            signature.computed,
                        ) else {
                            continue;
                        };
                        (
                            key,
                            DeclarationKind::Method,
                            signature.optional,
                            false,
                            Span::from_oxc(signature.span),
                        )
                    }
                    _ => continue,
                };
                let Some(surface) = prepared.object.property_by_key(&key) else {
                    continue;
                };
                let name = key.to_string();
                let owner = self
                    .lexical_events
                    .interface_occurrence_owner(
                        prepared.fragment.declaration,
                        InterfaceOccurrenceKind::Member,
                        span.start,
                    )
                    .expect("attached interface member has one exact owner");
                members.push(Member {
                    key,
                    name,
                    declaration_kind,
                    declaration_origin: DeclarationOrigin::Interface,
                    ty: surface.ty,
                    write_ty: surface.write_ty,
                    accessor_is_paired: false,
                    optional,
                    readonly,
                    visibility: Visibility::Public,
                    owner,
                    span,
                    order: (
                        fragment_source(prepared.fragment.declaration),
                        span.start,
                        prepared.fragment.declaration.0,
                    ),
                });
            }
        }
        members.sort_by_key(|member| (member.order, member.span.end));
        let mut by_name: BTreeMap<crate::types::repr::PropertyKey, Vec<Member<Ticket>>> =
            BTreeMap::new();
        let mut modifier_reports = BTreeSet::new();
        let mut duplicate_reports = BTreeSet::new();
        let mut accessibility_reports = BTreeSet::new();
        for member in members {
            let Some(first) = by_name
                .get(&member.key)
                .and_then(|occurrences| occurrences.first())
                .cloned()
            else {
                by_name.entry(member.key.clone()).or_default().push(member);
                continue;
            };
            if first.is_property_declaration() && member.conflicts_as_later_duplicate(&first) {
                for conflict in [&first, &member] {
                    if duplicate_reports.insert((
                        conflict.name.clone(),
                        conflict.order,
                        conflict.span.end,
                    )) {
                        self.with_ticket_effects(conflict.owner, |pass| {
                            pass.emit_diagnostic(Diagnostic::duplicate_identifier(
                                conflict.span,
                                &conflict.name,
                            ));
                        });
                    }
                }
            } else if first.declaration_kind == DeclarationKind::Method
                && member.declaration_kind == DeclarationKind::Method
            {
                if first.visibility != member.visibility
                    && accessibility_reports.insert((
                        member.name.clone(),
                        member.order,
                        member.span.end,
                    ))
                {
                    self.with_ticket_effects(member.owner, |pass| {
                        pass.emit_diagnostic(Diagnostic::overload_signatures_same_accessibility(
                            member.span,
                        ));
                    });
                }
            } else {
                if first.is_property_declaration()
                    && member.is_property_declaration()
                    && (first.optional != member.optional
                        || first.readonly != member.readonly
                        || first.visibility != member.visibility)
                {
                    for conflict in [&first, &member] {
                        if modifier_reports.insert((
                            conflict.name.clone(),
                            conflict.order,
                            conflict.span.end,
                        )) {
                            self.with_ticket_effects(conflict.owner, |pass| {
                                pass.emit_diagnostic(Diagnostic::identical_property_modifiers(
                                    conflict.span,
                                    &conflict.name,
                                ));
                            });
                        }
                    }
                }
                let first_ty = first.comparison_ty();
                let member_ty = member.comparison_ty();
                if first_ty != member_ty {
                    let source = self.interner.intern_object(crate::types::repr::ObjectType {
                        properties: vec![Self::public_signature_property(
                            member.key.clone(),
                            first_ty,
                        )],
                        ..Default::default()
                    });
                    let target = self.interner.intern_object(crate::types::repr::ObjectType {
                        properties: vec![Self::public_signature_property(
                            member.key.clone(),
                            member_ty,
                        )],
                        ..Default::default()
                    });
                    self.with_ticket_effects(member.owner, |pass| {
                        pass.schedule_interface_relation(InterfaceRelationObligation {
                            source,
                            target,
                            span: member.span,
                            kind: InterfaceRelationKind::MergedProperty {
                                name: member.name.clone(),
                            },
                            report: InterfaceRelationReport::Always,
                        });
                    });
                }
            }
            by_name.entry(member.key.clone()).or_default().push(member);
        }

        let derived_name = self
            .binder
            .type_groups
            .get(group)
            .map(|group| group.name.clone())
            .unwrap_or_else(|| "<class>".to_string());
        for prepared in interfaces {
            for (base_name, base) in &prepared.heritage_surfaces {
                self.validate_class_interface_heritage_surface(
                    group,
                    heritage_own,
                    prepared,
                    &derived_name,
                    base_name,
                    base,
                );
            }
        }
    }

    pub(in crate::check::checker) fn validate_class_interface_heritage_surface(
        &mut self,
        group: TypeGroupId,
        class_object: &crate::types::repr::ObjectType,
        prepared: &PreparedClassInterfaceFragment<'ast, Ticket>,
        derived_name: &str,
        base_name: &str,
        base: &crate::types::repr::ObjectType,
    ) {
        let span = self
            .binder
            .declarations
            .get(prepared.fragment.declaration)
            .map(|declaration| declaration.site.binding_span)
            .unwrap_or(Span::new(0, 0));
        let owner =
            self.merged_header_owner(group.index(), prepared.fragment.declaration, span.start);
        for class_property in &class_object.properties {
            let Some(base_property) = base.property_by_key(&class_property.key) else {
                continue;
            };
            if class_property.ty == base_property.ty
                && class_property.write_ty == base_property.write_ty
                && class_property.optional == base_property.optional
                && class_property.readonly == base_property.readonly
            {
                continue;
            }
            let source = self.interner.intern_object(crate::types::repr::ObjectType {
                properties: vec![class_property.clone()],
                ..Default::default()
            });
            let target = self.interner.intern_object(crate::types::repr::ObjectType {
                properties: vec![base_property.clone()],
                ..Default::default()
            });
            self.with_ticket_effects(owner, |pass| {
                pass.schedule_interface_relation(InterfaceRelationObligation {
                    source,
                    target,
                    span,
                    kind: InterfaceRelationKind::HeritageMember {
                        derived: derived_name.to_string(),
                        base: base_name.to_string(),
                    },
                    report: InterfaceRelationReport::Always,
                });
            });
        }
    }

    /// Construct ready interface components privately. Every member and heritage
    /// annotation in an SCC is lowered before any reserved root is filled.
    fn construct_pending_interface_sccs(&mut self, start: usize, end: usize) {
        let end = end.min(self.type_decls.len());
        let candidates = (start..end)
            .filter(|index| {
                matches!(
                    self.type_decls.get(*index),
                    Some(TypeDecl::Interface { .. })
                )
            })
            .collect::<Vec<_>>();
        self.construct_pending_interface_candidates(&candidates, start, end);
    }

    fn construct_pending_interface_candidates(
        &mut self,
        candidates: &[usize],
        work_start: usize,
        work_end: usize,
    ) {
        #[cfg(not(test))]
        let _ = (work_start, work_end);
        let has_pending_interface = candidates.iter().copied().any(|index| {
            u32::try_from(index)
                .ok()
                .is_some_and(|index| self.type_group_construction_is_pending(TypeGroupId(index)))
        });
        if !has_pending_interface {
            #[cfg(test)]
            INTERFACE_SCC_CONSTRUCTION_WORK.with(|work| {
                work.borrow_mut().push(InterfaceSccConstructionWork {
                    start: work_start,
                    end: work_end,
                    ..InterfaceSccConstructionWork::default()
                });
            });
            return;
        }
        #[cfg(test)]
        let topology_declaration_scans = self.type_decls.changed_entries().len();
        let topology = interface_heritage_topology(self.binder, &self.type_decls);
        #[cfg(test)]
        let scc_candidate_scans = candidates.len();
        let components = interface_sccs(&self.type_decls, candidates, &topology);
        let mut remaining: Vec<Vec<usize>> = components
            .into_iter()
            .filter(|component| {
                component.iter().any(|index| {
                    self.type_group_construction_is_pending(TypeGroupId(
                        u32::try_from(*index).expect("type group index fits u32"),
                    ))
                })
            })
            .collect();
        #[cfg(test)]
        let mut constructed_components = 0;

        loop {
            let mut progressed = false;
            let mut deferred = Vec::new();
            for component in remaining {
                if self.interface_component_is_ready(&component, &topology) {
                    let cyclic_heritage =
                        interface_component_has_cycle(&self.type_decls, &component, &topology);
                    self.construct_interface_component(&component, cyclic_heritage, &topology);
                    #[cfg(test)]
                    {
                        constructed_components += 1;
                    }
                    progressed = true;
                } else {
                    deferred.push(component);
                }
            }
            if !progressed || deferred.is_empty() {
                break;
            }
            remaining = deferred;
        }
        #[cfg(test)]
        INTERFACE_SCC_CONSTRUCTION_WORK.with(|work| {
            work.borrow_mut().push(InterfaceSccConstructionWork {
                start: work_start,
                end: work_end,
                topology_builds: 1,
                topology_declaration_scans,
                scc_builds: 1,
                scc_candidate_scans,
                constructed_components,
            });
        });
    }

    fn interface_component_is_ready(
        &self,
        component: &[usize],
        topology: &InterfaceHeritageTopology,
    ) -> bool {
        let members: FxHashSet<usize> = component.iter().copied().collect();
        component.iter().all(|&index| {
            let Some(TypeDecl::Interface { fragments, .. }) = self.type_decls.get(index) else {
                return false;
            };
            fragments.iter().all(|fragment| {
                fragment.extends.iter().all(|heritage| {
                    let InterfaceHeritagePlan::Complete(terminals) =
                        topology.plan(fragment.declaration, heritage)
                    else {
                        return true;
                    };
                    terminals.into_iter().all(|group| {
                        members.contains(&group.index())
                            || self.type_group_construction_is_frozen(group)
                    })
                })
            })
        })
    }

    fn construct_interface_component(
        &mut self,
        component: &[usize],
        cyclic_heritage: bool,
        topology: &InterfaceHeritageTopology,
    ) {
        for &index in component {
            self.begin_type_group_construction(TypeGroupId(
                u32::try_from(index).expect("type group index fits u32"),
            ));
            let state = self
                .template_fill
                .get_mut(index)
                .expect("interface component state");
            assert_eq!(
                *state,
                ClassFillState::Pending,
                "interface component {component:?} contains non-pending group {index}"
            );
            *state = ClassFillState::Filling;
        }

        let mut own_objects = Vec::with_capacity(component.len());
        for &index in component {
            let group = TypeGroupId(u32::try_from(index).expect("type group index fits u32"));
            let _replay_owner_scope = self
                .replay_trace
                .as_ref()
                .map(|trace| trace.scope(super::replay_index::ReplayOwner::TypeGroup(group)));
            let TypeDecl::Interface { fragments, .. } = &self.type_decls[index] else {
                unreachable!("interface SCC contains only interfaces")
            };
            let fragments = fragments.clone();
            self.validate_interface_group_headers(index, &fragments);
            if cyclic_heritage {
                self.report_cyclic_interface_heritage(
                    index,
                    &fragments,
                    component.len() > 1,
                    topology,
                );
            }
            let mut own = crate::types::repr::ObjectType::default();
            let mut unavailable = false;
            let mut first_method_members = BTreeSet::new();
            let mut lowered_fragments = Vec::with_capacity(fragments.len());
            for fragment in fragments {
                let frame = self.build_type_param_frame(fragment.param_decl, &fragment.params);
                let lowered =
                    self.with_exact_type_declaration_source(fragment.declaration, |pass| {
                        pass.with_type_params(frame, |pass| {
                            pass.lower_interface_declaration_members(
                                fragment.declaration,
                                fragment.scope,
                                fragment.members,
                            )
                        })
                    });
                let fragment_is_user = self
                    .binder
                    .declarations
                    .get(fragment.declaration)
                    .and_then(|declaration| {
                        self.binder.module_sources().get(&declaration.site.module)
                    })
                    .and_then(|source| {
                        self.binder
                            .namespaces
                            .compilation_origin_for_source(*source)
                    })
                    .is_some_and(|origin| {
                        matches!(origin, crate::source::CompilationOrigin::User(_))
                    });
                unavailable |= lowered.unavailable && fragment_is_user;
                let fragment_methods = lowered.method_keys;
                let fragment_own = lowered.object;
                lowered_fragments.push((fragment, fragment_own.clone()));
                own = self.merge_interface_fragment_members(
                    own,
                    fragment_own,
                    &mut first_method_members,
                    &fragment_methods,
                );
            }
            let alternatives = self.validate_interface_fragment_conflicts(&lowered_fragments);
            own_objects.push((index, own, alternatives, unavailable));
        }

        let component_set: FxHashSet<usize> = component.iter().copied().collect();
        let mut completed = Vec::with_capacity(component.len());
        let mut unavailable_groups = BTreeSet::new();
        for (index, own, mut alternatives, unavailable) in own_objects {
            let group = TypeGroupId(u32::try_from(index).expect("type group index fits u32"));
            if unavailable
                && (self
                    .private_collision_affected
                    .contains(&ReplayOwner::TypeGroup(group))
                    || self.combined_user_library_type_groups.contains(&group))
            {
                unavailable_groups.insert(group);
            }
            let _replay_owner_scope = self
                .replay_trace
                .as_ref()
                .map(|trace| trace.scope(super::replay_index::ReplayOwner::TypeGroup(group)));
            let TypeDecl::Interface { fragments, .. } = &self.type_decls[index] else {
                unreachable!()
            };
            let fragments = fragments.clone();
            let canonical_fragment = fragments
                .first()
                .expect("an interface group has at least one exact fragment");
            let canonical_span = self
                .binder
                .declarations
                .get(canonical_fragment.declaration)
                .map(|declaration| declaration.site.binding_span)
                .unwrap_or(Span::new(0, 0));
            let canonical_owner = self
                .lexical_events
                .interface_occurrence_owner(
                    canonical_fragment.declaration,
                    InterfaceOccurrenceKind::Header,
                    canonical_span.start,
                )
                .expect("canonical interface header has one exact preallocated owner");
            let mut heritage_surfaces = Vec::new();
            let own_owners = self.interface_own_member_owners(&fragments);
            for fragment in fragments {
                for heritage in fragment.extends {
                    let frame = self.build_type_param_frame(fragment.param_decl, &fragment.params);
                    let heritage_span = Span::from_oxc(heritage.span);
                    let owner = self
                        .lexical_events
                        .interface_occurrence_owner(
                            fragment.declaration,
                            InterfaceOccurrenceKind::Heritage,
                            heritage_span.start,
                        )
                        .expect("interface heritage has one exact preallocated owner");
                    let plan = topology.plan(fragment.declaration, heritage);
                    let internal = plan.terminals().is_some_and(|terminals| {
                        terminals
                            .iter()
                            .any(|group| component_set.contains(&group.index()))
                    });
                    if internal {
                        self.with_exact_type_declaration_source(fragment.declaration, |pass| {
                            pass.with_ticket_effects(owner, |pass| {
                                pass.with_type_params(frame, |pass| match plan {
                                    InterfaceHeritagePlan::Complete(_) => pass
                                        .validate_interface_heritage_application_without_resolution(
                                            fragment.scope,
                                            heritage,
                                        ),
                                    InterfaceHeritagePlan::Opaque(_) => pass
                                        .record_opaque_interface_heritage(fragment.scope, heritage),
                                    InterfaceHeritagePlan::Poisoned => {}
                                })
                            })
                        });
                        // Cyclic bases are invalid. Their annotations still own all
                        // diagnostics, but their members never cross the SCC boundary.
                        continue;
                    }
                    let base =
                        self.with_exact_type_declaration_source(fragment.declaration, |pass| {
                            pass.with_ticket_effects(owner, |pass| {
                                pass.with_type_params(frame, |pass| match plan {
                                    InterfaceHeritagePlan::Complete(_) => {
                                        pass.ensure_heritage_base_filled(fragment.scope, heritage);
                                        pass.resolve_interface_heritage_object(
                                            fragment.scope,
                                            heritage,
                                        )
                                    }
                                    InterfaceHeritagePlan::Poisoned => {
                                        pass.diagnose_poisoned_interface_heritage(
                                            fragment.scope,
                                            heritage,
                                        );
                                        None
                                    }
                                    InterfaceHeritagePlan::Opaque(_) => {
                                        pass.record_opaque_interface_heritage(
                                            fragment.scope,
                                            heritage,
                                        );
                                        None
                                    }
                                })
                            })
                        });
                    if let Some(base) = base {
                        heritage_surfaces.push((
                            owner,
                            heritage_span,
                            heritage_display_name(heritage),
                            base,
                        ));
                    }
                }
            }
            alternatives.extend(self.validate_interface_heritage_conflicts(
                &heritage_surfaces,
                &own,
                canonical_owner,
                canonical_span,
            ));
            let own_surface = own.clone();
            // Composing the whole chain at once keeps one name set over every base member.
            let bases = interface::compose_base_members_first(
                &heritage_surfaces
                    .iter()
                    .map(|(_, _, _, base)| base)
                    .collect::<Vec<_>>(),
            );
            let complete = interface::merge_object_members_overlay(bases, own);
            let derived_name = self
                .binder
                .type_groups
                .get(TypeGroupId(
                    u32::try_from(index).expect("type group index fits u32"),
                ))
                .map(|group| group.name.clone())
                .unwrap_or_else(|| "<interface>".to_string());
            alternatives.extend(self.validate_interface_heritage_indices(
                &complete,
                &heritage_surfaces,
                InterfaceHeritageDiagnostic {
                    owner: canonical_owner,
                    span: canonical_span,
                    derived_name: &derived_name,
                },
                &own_surface,
                &own_owners,
            ));
            let TypeDecl::Interface {
                conflict_alternatives,
                ..
            } = &mut self.type_decls[index]
            else {
                unreachable!()
            };
            *conflict_alternatives = alternatives;
            completed.push((index, complete));
        }

        let fills = completed
            .into_iter()
            .map(|(index, object)| {
                let TypeDecl::Interface { reserved, .. } = self.type_decls[index] else {
                    unreachable!()
                };
                crate::types::intern::ReservedTypeFill::Object(reserved, object)
            })
            .collect();
        self.interner
            .fill_reserved_type_batch(fills)
            .expect("an interface SCC freezes exactly once as one validated batch");
        for &index in component {
            let Some(group) = u32::try_from(index).ok().map(TypeGroupId) else {
                continue;
            };
            if unavailable_groups.contains(&group) {
                self.private_collision_unavailable_type_groups.insert(group);
            }
            self.template_fill[index] = ClassFillState::Done;
            self.freeze_type_group(group);
        }
    }

    fn report_cyclic_interface_heritage(
        &mut self,
        index: usize,
        fragments: &[InterfaceFragment<'ast>],
        report_every_fragment: bool,
        topology: &InterfaceHeritageTopology,
    ) {
        let name = self
            .binder
            .type_groups
            .get(TypeGroupId(
                u32::try_from(index).expect("type group index fits u32"),
            ))
            .map(|group| group.name.clone())
            .unwrap_or_else(|| "<interface>".to_string());
        for fragment in fragments {
            if !report_every_fragment
                && !fragment.extends.iter().any(|heritage| {
                    topology
                        .plan(fragment.declaration, heritage)
                        .terminals()
                        .is_some_and(|terminals| {
                            terminals.iter().any(|group| group.index() == index)
                        })
                })
            {
                continue;
            }
            let parameter_names = fragment
                .param_decl
                .iter()
                .flat_map(|declaration| declaration.params.iter())
                .map(|parameter| parameter.name.name.as_str())
                .collect::<Vec<_>>();
            let display = if parameter_names.is_empty() {
                name.clone()
            } else {
                format!("{}<{}>", name, parameter_names.join(", "))
            };
            let span = self
                .binder
                .declarations
                .get(fragment.declaration)
                .map(|declaration| declaration.site.binding_span)
                .unwrap_or(Span::new(0, 0));
            let owner = self
                .lexical_events
                .interface_occurrence_owner(
                    fragment.declaration,
                    InterfaceOccurrenceKind::Header,
                    span.start,
                )
                .expect("cyclic interface header has one exact preallocated owner");
            self.with_ticket_effects(owner, |pass| {
                pass.emit_diagnostic(crate::diagnostics::Diagnostic::circular_interface_heritage(
                    span, &display,
                ));
            });
        }
    }

    fn validate_interface_group_headers(
        &mut self,
        index: usize,
        fragments: &[InterfaceFragment<'ast>],
    ) {
        let Some(canonical_fragment) = fragments.first() else {
            return;
        };
        let canonical_descriptors = match &self.type_decls[index] {
            TypeDecl::Interface {
                parameter_descriptors,
                ..
            }
            | TypeDecl::Class {
                parameter_descriptors,
                ..
            } => parameter_descriptors
                .clone()
                .expect("canonical merged-header descriptors lower before group validation"),
            _ => unreachable!("header validation owns an interface or class draft"),
        };
        let mut shapes = Vec::with_capacity(fragments.len());
        shapes.push(
            canonical_fragment
                .param_decl
                .iter()
                .flat_map(|declaration| declaration.params.iter())
                .enumerate()
                .map(|(position, parameter)| {
                    (
                        parameter.name.name.to_string(),
                        canonical_descriptors
                            .constraints
                            .get(position)
                            .copied()
                            .unwrap_or(TypeParameterMetadataState::Absent),
                        canonical_descriptors
                            .defaults
                            .get(position)
                            .copied()
                            .unwrap_or(TypeParameterMetadataState::Absent),
                    )
                })
                .collect::<Vec<_>>(),
        );
        let mut supplied_defaults = Vec::new();
        for fragment in fragments.iter().skip(1) {
            let frame = self.build_type_param_frame(fragment.param_decl, &fragment.params);
            let header_span = self
                .binder
                .declarations
                .get(fragment.declaration)
                .map(|declaration| declaration.site.binding_span)
                .unwrap_or(Span::new(0, 0));
            let owner = self.merged_header_owner(index, fragment.declaration, header_span.start);
            let shape = self.with_exact_type_declaration_source(fragment.declaration, |pass| {
                pass.with_ticket_effects(owner, |pass| {
                    pass.with_type_params(frame, |pass| {
                        let descriptors = pass.lower_interface_fragment_parameter_descriptors(
                            fragment.scope,
                            fragment.param_decl,
                            &fragment.params,
                        );
                        fragment
                            .param_decl
                            .iter()
                            .flat_map(|declaration| declaration.params.iter())
                            .enumerate()
                            .map(|(position, parameter)| {
                                let constraint = descriptors
                                    .constraints
                                    .get(position)
                                    .copied()
                                    .unwrap_or(TypeParameterMetadataState::Absent);
                                let default = descriptors
                                    .defaults
                                    .get(position)
                                    .copied()
                                    .unwrap_or(TypeParameterMetadataState::Absent);
                                (parameter.name.name.to_string(), constraint, default)
                            })
                            .collect::<Vec<_>>()
                    })
                })
            });
            supplied_defaults.extend(
                fragment
                    .params
                    .iter()
                    .copied()
                    .zip(shape.iter())
                    .filter(|(_, (_, _, default))| default.is_supplied())
                    .map(|(parameter, (_, _, default))| {
                        let default = match default {
                            TypeParameterMetadataState::Ready(default) => {
                                PublishedTypeParameterDefault::Ready(*default)
                            }
                            TypeParameterMetadataState::Poisoned
                            | TypeParameterMetadataState::Unsupported => {
                                PublishedTypeParameterDefault::Unsupported
                            }
                            TypeParameterMetadataState::Absent => unreachable!(),
                        };
                        (parameter, default)
                    }),
            );
            shapes.push(shape);
        }
        let (recovery_params, recovery_defaults) = match &mut self.type_decls[index] {
            TypeDecl::Interface {
                recovery_params,
                recovery_defaults,
                ..
            } => (recovery_params, recovery_defaults),
            TypeDecl::Class {
                params,
                recovery_defaults,
                ..
            } => (params, recovery_defaults),
            _ => unreachable!("header validation owns an interface or class draft"),
        };
        for (parameter, default) in supplied_defaults {
            let recovery_index = recovery_params
                .iter()
                .position(|candidate| *candidate == parameter)
                .expect("fragment-local parameter is a recovery parameter");
            if recovery_defaults[recovery_index] == PublishedTypeParameterDefault::Absent {
                recovery_defaults[recovery_index] = default;
            }
        }
        let recovery_params = recovery_params.clone();
        let recovery_defaults = recovery_defaults.clone();
        let renamed_position =
            (0..shapes.iter().map(Vec::len).max().unwrap_or(0)).any(|position| {
                let mut names = shapes
                    .iter()
                    .filter_map(|shape| shape.get(position).map(|(name, ..)| name.as_str()));
                let first = names.next();
                names.any(|name| Some(name) != first)
            });
        let missing_required_extension =
            recovery_params
                .iter()
                .zip(recovery_defaults.iter())
                .any(|(parameter, default)| {
                    fragments
                        .iter()
                        .any(|fragment| !fragment.params.contains(parameter))
                        && *default == PublishedTypeParameterDefault::Absent
                });
        let group = TypeGroupId(u32::try_from(index).expect("type group index fits u32"));
        let name = self
            .binder
            .type_groups
            .get(group)
            .map(|group| group.name.clone())
            .unwrap_or_else(|| "<interface>".to_string());
        let immediate_header_mismatch = renamed_position || missing_required_extension;
        if immediate_header_mismatch {
            for declaration in fragments.iter().map(|fragment| fragment.declaration) {
                let span = self
                    .binder
                    .declarations
                    .get(declaration)
                    .map(|declaration| declaration.site.binding_span)
                    .unwrap_or(Span::new(0, 0));
                let owner = self.merged_header_owner(index, declaration, span.start);
                self.with_ticket_effects(owner, |pass| {
                    pass.emit_diagnostic(
                        crate::diagnostics::Diagnostic::merged_interface_type_parameters(
                            span, &name,
                        ),
                    );
                });
            }
        }

        let mut effective_constraints = FxHashMap::default();
        let mut effective_constraint_occurrences = FxHashMap::default();
        for parameter in &recovery_params {
            let occurrence = fragments.iter().zip(&shapes).enumerate().find_map(
                |(fragment_index, (fragment, shape))| {
                    fragment
                        .params
                        .iter()
                        .position(|candidate| candidate == parameter)
                        .and_then(|position| {
                            shape.get(position).and_then(|(_, constraint, _)| {
                                constraint
                                    .is_supplied()
                                    .then_some((*constraint, (fragment_index, position)))
                            })
                        })
                },
            );
            if let Some((constraint, occurrence)) = occurrence {
                effective_constraint_occurrences.insert(*parameter, occurrence);
                if let TypeParameterMetadataState::Ready(constraint) = constraint {
                    effective_constraints.insert(*parameter, constraint);
                }
            }
        }
        let cyclic_parameters = effective_constraints
            .keys()
            .copied()
            .filter(|parameter| {
                self.constraint_chain_revisits_with_overlay(*parameter, &effective_constraints)
            })
            .collect::<FxHashSet<_>>();
        for parameter in &recovery_params {
            self.interner.remove_type_param_constraint(*parameter);
            if !cyclic_parameters.contains(parameter) {
                if let Some(constraint) = effective_constraints.get(parameter).copied() {
                    let _ = self
                        .interner
                        .set_type_param_constraint(*parameter, constraint);
                }
            }
        }

        for (fragment_index, (fragment, shape)) in fragments.iter().zip(&shapes).enumerate() {
            let header_span = self
                .binder
                .declarations
                .get(fragment.declaration)
                .map(|declaration| declaration.site.binding_span)
                .unwrap_or(Span::new(0, 0));
            let owner = self.merged_header_owner(index, fragment.declaration, header_span.start);
            let parameters = fragment
                .param_decl
                .iter()
                .flat_map(|declaration| declaration.params.iter())
                .collect::<Vec<_>>();
            self.with_ticket_effects(owner, |pass| {
                for (position, ((parameter, descriptor), syntax)) in fragment
                    .params
                    .iter()
                    .copied()
                    .zip(shape)
                    .zip(&parameters)
                    .enumerate()
                {
                    let (_, constraint, default) = descriptor;
                    if matches!(constraint, TypeParameterMetadataState::Ready(_))
                        && cyclic_parameters.contains(&parameter)
                        && effective_constraint_occurrences.get(&parameter)
                            == Some(&(fragment_index, position))
                    {
                        let constraint_span = syntax
                            .constraint
                            .as_ref()
                            .map(|constraint| Span::from_oxc(constraint.span()))
                            .expect("supplied lowered constraint has syntax");
                        pass.emit_diagnostic(Diagnostic::circular_constraint(
                            constraint_span,
                            syntax.name.name.as_str(),
                        ));
                    }
                    if let TypeParameterMetadataState::Ready(default) = default {
                        let default_span = syntax
                            .default
                            .as_ref()
                            .map(|default| Span::from_oxc(default.span()))
                            .expect("supplied lowered default has syntax");
                        let effective_constraint = if cyclic_parameters.contains(&parameter) {
                            None
                        } else {
                            constraint
                                .ready()
                                .or_else(|| effective_constraints.get(&parameter).copied())
                        };
                        pass.check_constraint_arguments(
                            &[(effective_constraint, *default, default_span)],
                            &FxHashMap::default(),
                        );
                    }
                }
            });
        }

        if immediate_header_mismatch {
            return;
        }

        let mut identity_pairs = Vec::new();
        let mut definite_identity_mismatch = false;
        for parameter in recovery_params {
            let occurrences = fragments
                .iter()
                .zip(&shapes)
                .filter_map(|(fragment, shape)| {
                    fragment
                        .params
                        .iter()
                        .position(|candidate| *candidate == parameter)
                        .and_then(|position| shape.get(position))
                })
                .collect::<Vec<_>>();
            let constraint_roots = if cyclic_parameters.contains(&parameter) {
                Vec::new()
            } else {
                let states = occurrences
                    .iter()
                    .map(|(_, constraint, _)| *constraint)
                    .filter(|constraint| constraint.is_supplied())
                    .collect::<Vec<_>>();
                let poisoned = states.contains(&TypeParameterMetadataState::Poisoned);
                let ready = states
                    .iter()
                    .filter_map(|state| state.ready())
                    .collect::<Vec<_>>();
                if poisoned && !ready.is_empty() {
                    definite_identity_mismatch = true;
                }
                ready
            };
            let default_states = occurrences
                .iter()
                .map(|(_, _, default)| *default)
                .filter(|default| default.is_supplied())
                .collect::<Vec<_>>();
            let poisoned_default = default_states.contains(&TypeParameterMetadataState::Poisoned);
            let default_roots = default_states
                .iter()
                .filter_map(|state| state.ready())
                .collect::<Vec<_>>();
            if poisoned_default && !default_roots.is_empty() {
                definite_identity_mismatch = true;
            }
            for roots in [constraint_roots, default_roots] {
                let Some(first) = roots.first().copied() else {
                    continue;
                };
                identity_pairs.extend(
                    roots
                        .into_iter()
                        .skip(1)
                        .filter(|candidate| *candidate != first)
                        .map(|candidate| (first, candidate)),
                );
            }
        }
        if definite_identity_mismatch {
            for declaration in fragments.iter().map(|fragment| fragment.declaration) {
                let span = self
                    .binder
                    .declarations
                    .get(declaration)
                    .map(|declaration| declaration.site.binding_span)
                    .unwrap_or(Span::new(0, 0));
                let owner = self.merged_header_owner(index, declaration, span.start);
                self.with_ticket_effects(owner, |pass| {
                    pass.emit_diagnostic(
                        crate::diagnostics::Diagnostic::merged_interface_type_parameters(
                            span, &name,
                        ),
                    );
                });
            }
            return;
        }
        if identity_pairs.is_empty() {
            return;
        }
        for fragment in fragments {
            let declaration = fragment.declaration;
            let span = self
                .binder
                .declarations
                .get(declaration)
                .map(|declaration| declaration.site.binding_span)
                .unwrap_or(Span::new(0, 0));
            let owner = self.merged_header_owner(index, declaration, span.start);
            self.with_ticket_effects(owner, |pass| {
                for &(source, target) in &identity_pairs {
                    pass.schedule_interface_relation(InterfaceRelationObligation {
                        source,
                        target,
                        span,
                        kind: InterfaceRelationKind::HeaderMetadata { name: name.clone() },
                        report: InterfaceRelationReport::FirstFailedHeaderGroup(group),
                    });
                }
            });
        }
    }

    fn validate_interface_fragment_conflicts(
        &mut self,
        fragments: &[(InterfaceFragment<'ast>, crate::types::repr::ObjectType)],
    ) -> Vec<InterfaceTypedAlternative> {
        #[derive(Copy, Clone, PartialEq, Eq)]
        enum MemberKind {
            Property,
            Method,
        }
        #[derive(Clone)]
        struct Member<Ticket: Copy> {
            owner: Ticket,
            span: Span,
            kind: MemberKind,
            ty: TypeId,
            optional: bool,
            readonly: bool,
            name: String,
        }
        #[derive(Clone)]
        struct Index<Ticket: Copy> {
            owner: Ticket,
            span: Span,
            ty: TypeId,
        }

        let mut members: BTreeMap<crate::types::repr::PropertyKey, Vec<Member<Ticket>>> =
            BTreeMap::new();
        let mut all_properties = Vec::new();
        let mut string_index: Option<Index<Ticket>> = None;
        let mut number_index: Option<Index<Ticket>> = None;
        let mut string_indices = Vec::new();
        let mut number_indices = Vec::new();
        let mut records = Vec::new();
        let mut relations = Vec::new();
        let mut reported_duplicate_members = BTreeSet::new();
        let mut reported_modifier_members = BTreeSet::new();
        let mut reported_duplicate_indices = BTreeSet::new();
        for (fragment, object) in fragments {
            for signature in fragment.members {
                match signature {
                    oxc_ast::ast::TSSignature::TSPropertySignature(signature) => {
                        let Some(key) = self.signature_property_key(
                            fragment.scope,
                            &signature.key,
                            signature.computed,
                        ) else {
                            continue;
                        };
                        let Some(property) = object.property_by_key(&key) else {
                            continue;
                        };
                        let name = key.to_string();
                        let span = Span::from_oxc(signature.span);
                        let member = Member {
                            owner: self
                                .lexical_events
                                .interface_occurrence_owner(
                                    fragment.declaration,
                                    InterfaceOccurrenceKind::Member,
                                    span.start,
                                )
                                .expect("interface member has one exact preallocated owner"),
                            span,
                            kind: MemberKind::Property,
                            ty: property.ty,
                            optional: signature.optional,
                            readonly: signature.readonly,
                            name: name.clone(),
                        };
                        if key.as_string().is_some() {
                            all_properties.push(member.clone());
                        }
                        if let Some(first) = members.get(&key).and_then(|items| items.first()) {
                            if first.kind != member.kind {
                                if first.ty != member.ty {
                                    let source = self.interner.intern_object(
                                        crate::types::repr::ObjectType {
                                            properties: vec![Self::public_signature_property(
                                                key.clone(),
                                                first.ty,
                                            )],
                                            ..Default::default()
                                        },
                                    );
                                    let target = self.interner.intern_object(
                                        crate::types::repr::ObjectType {
                                            properties: vec![Self::public_signature_property(
                                                key.clone(),
                                                member.ty,
                                            )],
                                            ..Default::default()
                                        },
                                    );
                                    relations.push((
                                        member.owner,
                                        InterfaceRelationObligation {
                                            source,
                                            target,
                                            span: member.span,
                                            kind: InterfaceRelationKind::MergedProperty {
                                                name: name.clone(),
                                            },
                                            report: InterfaceRelationReport::Always,
                                        },
                                    ));
                                }
                            } else {
                                if first.optional != member.optional
                                    || first.readonly != member.readonly
                                {
                                    for conflict in [first, &member] {
                                        if reported_modifier_members.insert((
                                            name.clone(),
                                            conflict.span.start,
                                            conflict.span.end,
                                        )) {
                                            records.push((
                                                conflict.owner,
                                                crate::diagnostics::Diagnostic::identical_property_modifiers(
                                                    conflict.span,
                                                    &name,
                                                ),
                                            ));
                                        }
                                    }
                                }
                                if first.ty != member.ty {
                                    let mut first_property =
                                        Self::public_signature_property(key.clone(), first.ty);
                                    first_property.optional = first.optional;
                                    first_property.readonly = first.readonly;
                                    let mut later_property =
                                        Self::public_signature_property(key.clone(), member.ty);
                                    later_property.optional = member.optional;
                                    later_property.readonly = member.readonly;
                                    let source = self.interner.intern_object(
                                        crate::types::repr::ObjectType {
                                            properties: vec![first_property],
                                            ..Default::default()
                                        },
                                    );
                                    let target = self.interner.intern_object(
                                        crate::types::repr::ObjectType {
                                            properties: vec![later_property],
                                            ..Default::default()
                                        },
                                    );
                                    relations.push((
                                        member.owner,
                                        InterfaceRelationObligation {
                                            source,
                                            target,
                                            span: member.span,
                                            kind: InterfaceRelationKind::MergedProperty {
                                                name: name.clone(),
                                            },
                                            report: InterfaceRelationReport::Always,
                                        },
                                    ));
                                }
                            }
                        }
                        members.entry(key).or_default().push(member);
                    }
                    oxc_ast::ast::TSSignature::TSMethodSignature(signature) => {
                        let Some(key) = self.signature_property_key(
                            fragment.scope,
                            &signature.key,
                            signature.computed,
                        ) else {
                            continue;
                        };
                        let Some(property) = object.property_by_key(&key) else {
                            continue;
                        };
                        let name = key.to_string();
                        let span = Span::from_oxc(signature.span);
                        let member = Member {
                            owner: self
                                .lexical_events
                                .interface_occurrence_owner(
                                    fragment.declaration,
                                    InterfaceOccurrenceKind::Member,
                                    span.start,
                                )
                                .expect("interface member has one exact preallocated owner"),
                            span,
                            kind: MemberKind::Method,
                            ty: property.ty,
                            optional: signature.optional,
                            readonly: false,
                            name: name.clone(),
                        };
                        if key.as_string().is_some() {
                            all_properties.push(member.clone());
                        }
                        if let Some(first) = members.get(&key).and_then(|items| items.first()) {
                            if first.kind != member.kind {
                                for conflict in [first, &member] {
                                    if reported_duplicate_members.insert((
                                        name.clone(),
                                        conflict.span.start,
                                        conflict.span.end,
                                    )) {
                                        records.push((
                                            conflict.owner,
                                            crate::diagnostics::Diagnostic::duplicate_identifier(
                                                conflict.span,
                                                &name,
                                            ),
                                        ));
                                    }
                                }
                            }
                        }
                        members.entry(key).or_default().push(member);
                    }
                    oxc_ast::ast::TSSignature::TSIndexSignature(signature) => {
                        let key = signature
                            .parameters
                            .first()
                            .map(|parameter| &parameter.type_annotation.type_annotation);
                        let (slot, key_name, ty) = match key {
                            Some(oxc_ast::ast::TSType::TSStringKeyword(_)) => {
                                (&mut string_index, "string", object.string_index)
                            }
                            Some(oxc_ast::ast::TSType::TSNumberKeyword(_)) => {
                                (&mut number_index, "number", object.number_index)
                            }
                            _ => continue,
                        };
                        let Some(ty) = ty else { continue };
                        let span = Span::from_oxc(signature.span);
                        let current = Index {
                            owner: self
                                .lexical_events
                                .interface_occurrence_owner(
                                    fragment.declaration,
                                    InterfaceOccurrenceKind::Member,
                                    span.start,
                                )
                                .expect("interface index has one exact preallocated owner"),
                            span,
                            ty,
                        };
                        if let Some(first) = slot.as_ref() {
                            for conflict in [first, &current] {
                                if reported_duplicate_indices.insert((
                                    key_name,
                                    conflict.span.start,
                                    conflict.span.end,
                                )) {
                                    records.push((
                                        conflict.owner,
                                        crate::diagnostics::Diagnostic::duplicate_index_signature(
                                            conflict.span,
                                            key_name,
                                        ),
                                    ));
                                }
                            }
                        } else {
                            *slot = Some(current.clone());
                        }
                        if key_name == "string" {
                            string_indices.push(current);
                        } else {
                            number_indices.push(current);
                        }
                    }
                    _ => {}
                }
            }
        }

        if let (Some(string), Some(number)) = (&string_index, &number_index) {
            relations.push((
                number.owner,
                InterfaceRelationObligation {
                    source: number.ty,
                    target: string.ty,
                    span: number.span,
                    kind: InterfaceRelationKind::NumberIndex,
                    report: InterfaceRelationReport::Always,
                },
            ));
        }
        if let Some(string) = &string_index {
            for property in all_properties {
                relations.push((
                    property.owner,
                    InterfaceRelationObligation {
                        source: property.ty,
                        target: string.ty,
                        span: property.span,
                        kind: InterfaceRelationKind::PropertyStringIndex {
                            name: property.name,
                        },
                        report: InterfaceRelationReport::Always,
                    },
                ));
            }
        }
        for (owner, diagnostic) in records {
            self.with_ticket_effects(owner, |pass| pass.emit_diagnostic(diagnostic));
        }
        for (owner, relation) in relations {
            self.with_ticket_effects(owner, |pass| pass.schedule_interface_relation(relation));
        }
        let mut alternatives: Vec<InterfaceTypedAlternative> = members
            .into_iter()
            .filter_map(|(key, occurrences)| {
                (occurrences.len() > 1).then(|| InterfaceTypedAlternative {
                    kind: InterfaceAlternativeKind::Member,
                    key: key.to_string(),
                    types: occurrences
                        .into_iter()
                        .map(|occurrence| occurrence.ty)
                        .collect(),
                })
            })
            .collect();
        for (kind, key, occurrences) in [
            (
                InterfaceAlternativeKind::StringIndex,
                "string",
                string_indices,
            ),
            (
                InterfaceAlternativeKind::NumberIndex,
                "number",
                number_indices,
            ),
        ] {
            if occurrences.len() > 1 {
                alternatives.push(InterfaceTypedAlternative {
                    kind,
                    key: key.to_string(),
                    types: occurrences
                        .into_iter()
                        .map(|occurrence| occurrence.ty)
                        .collect(),
                });
            }
        }
        alternatives
    }

    fn validate_interface_heritage_conflicts(
        &mut self,
        surfaces: &[(Ticket, Span, String, crate::types::repr::ObjectType)],
        own: &crate::types::repr::ObjectType,
        diagnostic_owner: Ticket,
        diagnostic_span: Span,
    ) -> Vec<InterfaceTypedAlternative> {
        let mut alternatives = Vec::new();
        let mut pair_ordinal = 0_u32;
        for (index, (_, _, left_name, left)) in surfaces.iter().enumerate() {
            for (_, _, right_name, right) in surfaces.iter().skip(index + 1) {
                let pair = pair_ordinal;
                pair_ordinal = pair_ordinal
                    .checked_add(1)
                    .expect("interface heritage pair ordinal fits u32");
                for left_property in &left.properties {
                    let Some(right_property) = right.property_by_key(&left_property.key) else {
                        continue;
                    };
                    // A complete own property replaces the inherited candidates.
                    if own.property_by_key(&left_property.key).is_some() {
                        continue;
                    }
                    if left_property.ty == right_property.ty
                        && left_property.write_ty == right_property.write_ty
                        && left_property.optional == right_property.optional
                        && left_property.visibility == right_property.visibility
                        && left_property.declaring_class == right_property.declaring_class
                        && left_property.readonly == right_property.readonly
                        && left_property.is_accessor == right_property.is_accessor
                    {
                        continue;
                    }
                    let source = self.interner.intern_object(crate::types::repr::ObjectType {
                        properties: vec![left_property.clone()],
                        ..Default::default()
                    });
                    let target = self.interner.intern_object(crate::types::repr::ObjectType {
                        properties: vec![right_property.clone()],
                        ..Default::default()
                    });
                    alternatives.push(InterfaceTypedAlternative {
                        kind: InterfaceAlternativeKind::Heritage,
                        key: left_property.key.to_string(),
                        types: vec![left_property.ty, right_property.ty],
                    });
                    self.with_ticket_effects(diagnostic_owner, |pass| {
                        pass.schedule_interface_relation(InterfaceRelationObligation {
                            source,
                            target,
                            span: diagnostic_span,
                            kind: InterfaceRelationKind::Heritage {
                                left: left_name.clone(),
                                right: right_name.clone(),
                            },
                            report: InterfaceRelationReport::FirstFailedHeritagePair(pair),
                        });
                    });
                }
            }
        }
        alternatives
    }

    fn interface_own_member_owners(
        &self,
        fragments: &[InterfaceFragment<'ast>],
    ) -> InterfaceOwnMemberOwners<Ticket> {
        let mut owners = InterfaceOwnMemberOwners::default();
        for fragment in fragments {
            for member in fragment.members {
                let span = Span::from_oxc(member.span());
                let owner = self
                    .lexical_events
                    .interface_occurrence_owner(
                        fragment.declaration,
                        InterfaceOccurrenceKind::Member,
                        span.start,
                    )
                    .expect("interface member has one exact preallocated owner");
                match member {
                    oxc_ast::ast::TSSignature::TSPropertySignature(signature) => {
                        if let Some(key) = self.signature_property_key(
                            fragment.scope,
                            &signature.key,
                            signature.computed,
                        ) {
                            owners.properties.entry(key).or_insert((owner, span));
                        }
                    }
                    oxc_ast::ast::TSSignature::TSMethodSignature(signature) => {
                        if let Some(key) = self.signature_property_key(
                            fragment.scope,
                            &signature.key,
                            signature.computed,
                        ) {
                            owners.properties.entry(key).or_insert((owner, span));
                        }
                    }
                    oxc_ast::ast::TSSignature::TSIndexSignature(signature) => {
                        let slot = match signature
                            .parameters
                            .first()
                            .map(|parameter| &parameter.type_annotation.type_annotation)
                        {
                            Some(oxc_ast::ast::TSType::TSStringKeyword(_)) => {
                                &mut owners.string_index
                            }
                            Some(oxc_ast::ast::TSType::TSNumberKeyword(_)) => {
                                &mut owners.number_index
                            }
                            _ => continue,
                        };
                        if slot.is_none() {
                            *slot = Some((owner, span));
                        }
                    }
                    _ => {}
                }
            }
        }
        owners
    }

    fn validate_interface_heritage_indices(
        &mut self,
        complete: &crate::types::repr::ObjectType,
        surfaces: &[(Ticket, Span, String, crate::types::repr::ObjectType)],
        diagnostic: InterfaceHeritageDiagnostic<'_, Ticket>,
        own: &crate::types::repr::ObjectType,
        own_owners: &InterfaceOwnMemberOwners<Ticket>,
    ) -> Vec<InterfaceTypedAlternative> {
        let mut alternatives = Vec::new();
        for (_, _, base_name, base) in surfaces {
            for own_property in &own.properties {
                let Some(base_property) = base.property_by_key(&own_property.key) else {
                    continue;
                };
                if own_property.ty == base_property.ty
                    && own_property.write_ty == base_property.write_ty
                    && own_property.optional == base_property.optional
                    && own_property.visibility == base_property.visibility
                    && own_property.declaring_class == base_property.declaring_class
                    && own_property.readonly == base_property.readonly
                    && own_property.is_accessor == base_property.is_accessor
                {
                    continue;
                }
                let source = self.interner.intern_object(crate::types::repr::ObjectType {
                    properties: vec![own_property.clone()],
                    ..Default::default()
                });
                let target = self.interner.intern_object(crate::types::repr::ObjectType {
                    properties: vec![base_property.clone()],
                    ..Default::default()
                });
                alternatives.push(InterfaceTypedAlternative {
                    kind: InterfaceAlternativeKind::Heritage,
                    key: own_property.key.to_string(),
                    types: vec![own_property.ty, base_property.ty],
                });
                self.with_ticket_effects(diagnostic.owner, |pass| {
                    pass.schedule_interface_relation(InterfaceRelationObligation {
                        source,
                        target,
                        span: diagnostic.span,
                        kind: InterfaceRelationKind::HeritageMember {
                            derived: diagnostic.derived_name.to_string(),
                            base: base_name.clone(),
                        },
                        report: InterfaceRelationReport::Always,
                    });
                });
            }
        }
        for (kind, key, source, own_index) in [
            (
                InterfaceAlternativeKind::StringIndex,
                "string",
                complete.string_index,
                own.string_index,
            ),
            (
                InterfaceAlternativeKind::NumberIndex,
                "number",
                complete.number_index,
                own.number_index,
            ),
        ] {
            let ordered_bases: Vec<_> = if own_index.is_some() {
                surfaces.iter().rev().collect()
            } else {
                surfaces.iter().collect()
            };
            for (_, _, base_name, base) in ordered_bases {
                let target = if kind == InterfaceAlternativeKind::StringIndex {
                    base.string_index
                } else {
                    base.number_index
                };
                let (Some(source), Some(target)) = (source, target) else {
                    continue;
                };
                if source == target {
                    continue;
                }
                alternatives.push(InterfaceTypedAlternative {
                    kind,
                    key: key.to_string(),
                    types: vec![source, target],
                });
                self.with_ticket_effects(diagnostic.owner, |pass| {
                    pass.schedule_interface_relation(InterfaceRelationObligation {
                        source,
                        target,
                        span: diagnostic.span,
                        kind: InterfaceRelationKind::HeritageIndex {
                            derived: diagnostic.derived_name.to_string(),
                            base: base_name.clone(),
                        },
                        report: InterfaceRelationReport::Always,
                    });
                });
            }
        }
        if let Some(string_index) = complete.string_index {
            for property in &complete.properties {
                let Some(name) = property.key.as_string() else {
                    continue;
                };
                let own_property = own_owners.properties.get(&property.key).copied();
                let own_string = own_owners.string_index;
                if own_property.is_some() && own_string.is_some() {
                    continue;
                }
                let (owner, span) = own_property
                    .or(own_string)
                    .unwrap_or((diagnostic.owner, diagnostic.span));
                self.with_ticket_effects(owner, |pass| {
                    pass.schedule_interface_relation(InterfaceRelationObligation {
                        source: property.ty,
                        target: string_index,
                        span,
                        kind: InterfaceRelationKind::PropertyStringIndex {
                            name: name.to_owned(),
                        },
                        report: InterfaceRelationReport::Always,
                    });
                });
            }
        }
        if let (Some(number_index), Some(string_index)) =
            (complete.number_index, complete.string_index)
        {
            let own_number = own_owners.number_index;
            let own_string = own_owners.string_index;
            if own_number.is_none() || own_string.is_none() {
                let (owner, span) = own_number
                    .or(own_string)
                    .unwrap_or((diagnostic.owner, diagnostic.span));
                self.with_ticket_effects(owner, |pass| {
                    pass.schedule_interface_relation(InterfaceRelationObligation {
                        source: number_index,
                        target: string_index,
                        span,
                        kind: InterfaceRelationKind::NumberIndex,
                        report: InterfaceRelationReport::Always,
                    });
                });
            }
        }
        alternatives
    }

    /// Fill one seeded object-literal alias's reserved object with lowered members.
    /// Runs on demand in `template_fill`; `resolving_alias` stays set so nested
    /// mapped self-references still report `TK2456`.
    fn ensure_object_alias_filled(&mut self, _scope: ScopeId, index: usize) {
        if !matches!(self.template_fill.get(index), Some(ClassFillState::Pending)) {
            return;
        }
        let decl_id = TypeGroupId(u32::try_from(index).expect("type group index fits u32"));
        let filled = self
            .with_type_decl_effects(decl_id, |pass| pass.ensure_object_alias_filled_inner(index));
        if filled {
            self.freeze_type_group(decl_id);
        }
    }

    fn ensure_object_alias_filled_inner(&mut self, index: usize) -> bool {
        match self.template_fill.get(index).copied() {
            Some(ClassFillState::Done) | Some(ClassFillState::Filling) => return false,
            Some(ClassFillState::Pending) => {}
            None => return false,
        }
        let (scope, reserved, members, name, name_span) = match &self.type_decls[index] {
            TypeDecl::Alias {
                scope,
                object_template: Some(reserved),
                annotation: oxc_ast::ast::TSType::TSTypeLiteral(lit),
                name,
                name_span,
                ..
            } => (*scope, *reserved, &lit.members, name.clone(), *name_span),
            // Not a seeded object alias (a Pending interface belongs to
            // [`ensure_interface_filled`]) — leave the state untouched.
            _ => return false,
        };
        if let Some(slot) = self.template_fill.get_mut(index) {
            *slot = ClassFillState::Filling;
        }
        let decl_id = TypeGroupId(u32::try_from(index).expect("type group index fits u32"));
        self.begin_type_group_construction(decl_id);
        let prev_resolving_alias = self.resolving_alias.take();
        self.resolving_alias = Some((decl_id, name_span, name.clone()));
        self.resolving_alias_stack
            .push((decl_id, name_span, name, self.alias_indirection_depth));
        let object = self.lower_interface_members(scope, members);
        self.resolving_alias_stack.pop();
        self.resolving_alias = prev_resolving_alias;
        self.interner.fill_object(reserved, object);
        if let Some(slot) = self.template_fill.get_mut(index) {
            *slot = ClassFillState::Done;
        }
        true
    }

    fn object_alias_semantic_roots(&self, active_owners: &FxHashMap<TypeId, usize>) -> Vec<TypeId> {
        let mut roots = Vec::new();
        for index in self.type_decls.published_len()..self.type_decls.len() {
            let Some(resolved) = self.type_resolved.get(index).and_then(|resolved| *resolved)
            else {
                continue;
            };
            if active_owners.get(&resolved).copied() != Some(index) {
                roots.push(resolved);
            }
        }

        for declaration in self.type_decls.iter() {
            match declaration {
                TypeDecl::Interface {
                    defaults,
                    recovery_defaults,
                    conflict_alternatives,
                    parameter_descriptors,
                    ..
                } => {
                    roots.extend(defaults.iter().flatten().copied());
                    push_published_defaults(&mut roots, recovery_defaults);
                    push_typed_alternatives(&mut roots, conflict_alternatives);
                    push_type_parameter_descriptors(&mut roots, parameter_descriptors.as_ref());
                }
                TypeDecl::Alias { defaults, .. } => {
                    roots.extend(defaults.iter().flatten().copied());
                }
                TypeDecl::Class {
                    recovery_defaults,
                    conflict_alternatives,
                    parameter_descriptors,
                    ..
                } => {
                    push_published_defaults(&mut roots, recovery_defaults);
                    push_typed_alternatives(&mut roots, conflict_alternatives);
                    push_type_parameter_descriptors(&mut roots, parameter_descriptors.as_ref());
                }
                TypeDecl::Unavailable { .. } | TypeDecl::Resolved { .. } => {}
            }
        }

        for effects in &self.pending_effects {
            push_pending_effect_type_roots(&mut roots, effects);
        }
        for ((template, arguments), result) in &self.eager_application_cache {
            roots.push(*template);
            roots.extend(arguments.iter().map(|(_, argument)| *argument));
            roots.push(*result);
        }
        roots
    }

    fn fill_and_canonicalize_object_aliases_range(&mut self, start: usize, end: usize) {
        let candidates = (start..end.min(self.type_decls.len()))
            .filter(|index| {
                matches!(
                    (self.template_fill.get(*index), self.type_decls.get(*index)),
                    (
                        Some(ClassFillState::Pending),
                        Some(TypeDecl::Alias {
                            object_template: Some(_),
                            ..
                        })
                    )
                )
            })
            .collect::<Vec<_>>();
        let mut filled = Vec::with_capacity(candidates.len());
        for index in candidates {
            let decl_id = TypeGroupId(u32::try_from(index).expect("type group index fits u32"));
            if self.with_type_decl_effects(decl_id, |pass| {
                pass.ensure_object_alias_filled_inner(index)
            }) {
                filled.push(index);
            }
        }
        if filled.is_empty() {
            return;
        }

        let active_owners = self
            .type_decls
            .iter()
            .enumerate()
            .map(|(index, declaration)| (self.type_decls.published_len() + index, declaration))
            .filter_map(|(index, declaration)| match declaration {
                TypeDecl::Alias {
                    object_template: Some(reserved),
                    ..
                } => Some((*reserved, index)),
                _ => None,
            })
            .collect::<FxHashMap<_, _>>();
        let active = active_owners.keys().copied().collect::<FxHashSet<_>>();
        let graph = ObjectAliasCanonicalizationGraph::build(self.interner.store(), &active);
        let semantic_roots = self.object_alias_semantic_roots(&active_owners);
        let externally_captured = graph.active_reservations_reachable_from_semantic_roots(
            self.interner.store(),
            semantic_roots,
            &active,
        );

        let mut promotion_order = Vec::with_capacity(filled.len());
        // Fixed semantic roots may anchor dedup only when they retain their exact id.
        for captured in [true, false] {
            promotion_order.extend(filled.iter().copied().filter(|index| {
                let TypeDecl::Alias {
                    object_template: Some(reserved),
                    ..
                } = self.type_decls[*index]
                else {
                    return false;
                };
                externally_captured.contains(&reserved) == captured
            }));
        }

        for index in promotion_order {
            let reserved = match &self.type_decls[index] {
                TypeDecl::Alias {
                    object_template: Some(reserved),
                    ..
                } => *reserved,
                _ => continue,
            };
            let safe = !graph.has_external_store_inbound(reserved)
                && !graph.body_reaches_active_reservation(self.interner.store(), reserved);
            if !safe {
                continue;
            }
            let canonical = self
                .interner
                .promote_caller_certified_acyclic_reserved_object(reserved)
                .expect("caller-certified acyclic object alias owns one frozen reservation");
            // A collision leaves the reservation untouched, so captured holders stay valid.
            if externally_captured.contains(&reserved) && canonical != reserved {
                continue;
            }
            self.type_resolved[index] = Some(canonical);
            let TypeDecl::Alias {
                object_template, ..
            } = &mut self.type_decls[index]
            else {
                unreachable!("canonicalized object alias retains its declaration draft")
            };
            *object_template = None;
        }

        for index in filled {
            self.freeze_type_group(TypeGroupId(
                u32::try_from(index).expect("type group index fits u32"),
            ));
        }
    }

    /// Force-fill a heritage base before composition reads its members.
    /// Interfaces recurse, while aliases resolve then fill any reserved template
    /// they land on. Generic alias heritage resolves on demand at instantiation time.
    fn ensure_heritage_base_filled(&mut self, scope: ScopeId, heritage: &TSInterfaceHeritage<'_>) {
        let Some((decl_id, _, _)) = self.interface_heritage_root_replay(scope, heritage) else {
            return;
        };
        match self.type_decls.get(decl_id.index()) {
            // Direct interface dependencies are scheduled by the SCC graph.
            Some(TypeDecl::Interface { .. }) => {}
            Some(TypeDecl::Class { .. }) => {}
            Some(TypeDecl::Alias { .. }) if heritage.type_arguments.is_none() => {
                let ty = self.resolve_type_decl(scope, decl_id);
                self.ensure_reserved_template_filled(scope, ty);
            }
            _ => {}
        }
    }

    /// Fill the reserved-template declaration owned by a resolved base `TypeId`, if any.
    /// Transparent alias chains then compose the filled target in any declaration order.
    fn ensure_reserved_template_filled(&mut self, scope: ScopeId, ty: TypeId) {
        let target = self
            .type_decls
            .iter()
            .position(|decl| match decl {
                TypeDecl::Interface { reserved, .. } => *reserved == ty,
                TypeDecl::Alias {
                    object_template: Some(reserved),
                    ..
                } => *reserved == ty,
                _ => false,
            })
            .map(|index| self.type_decls.published_len() + index);
        let Some(index) = target else {
            return;
        };
        match self.type_decls.get(index) {
            Some(TypeDecl::Interface { .. }) => {}
            Some(TypeDecl::Alias { .. }) => self.ensure_object_alias_filled(scope, index),
            _ => {}
        }
    }
}

fn interface_heritage_topology(
    binder: &Binder,
    declarations: &TypeDeclTable<'_>,
) -> InterfaceHeritageTopology {
    let mut topology = InterfaceHeritageTopology::default();
    for (_, declaration) in declarations.changed_entries() {
        let fragments = match declaration {
            TypeDecl::Interface { fragments, .. } => fragments,
            TypeDecl::Class { interfaces, .. } => interfaces,
            _ => continue,
        };
        for fragment in fragments {
            let symbols = fragment
                .param_decl
                .iter()
                .flat_map(|parameters| parameters.params.iter())
                .map(|parameter| {
                    (
                        parameter.name.name.to_string(),
                        HeritageTypePlan::absorber(IntersectionAbsorber::Unknown),
                    )
                })
                .collect();
            for heritage in fragment.extends {
                let plan = plan_heritage_occurrence(
                    binder,
                    declarations,
                    fragment.scope,
                    heritage,
                    &symbols,
                    &mut BTreeSet::new(),
                );
                topology.occurrences.insert(
                    (fragment.declaration, heritage.span.start),
                    plan.into_topology_plan(),
                );
            }
        }
    }
    topology
}

fn plan_heritage_occurrence(
    binder: &Binder,
    declarations: &TypeDeclTable<'_>,
    scope: ScopeId,
    heritage: &TSInterfaceHeritage<'_>,
    symbols: &BTreeMap<String, HeritageTypePlan>,
    aliases: &mut BTreeSet<TypeGroupId>,
) -> HeritageTypePlan {
    if let Expression::Identifier(identifier) = &heritage.expression {
        if let Some(symbol) = symbols.get(identifier.name.as_str()) {
            return if heritage.type_arguments.is_none() {
                symbol.clone()
            } else {
                HeritageTypePlan::Poisoned
            };
        }
    }
    let group = match topology_heritage_group(binder, scope, heritage) {
        Ok(group) => group,
        Err(plan) => return plan,
    };
    plan_heritage_group_application(
        binder,
        declarations,
        scope,
        group,
        heritage.type_arguments.as_deref(),
        symbols,
        aliases,
    )
}

fn plan_heritage_type(
    binder: &Binder,
    declarations: &TypeDeclTable<'_>,
    scope: ScopeId,
    ty: &TSType<'_>,
    symbols: &BTreeMap<String, HeritageTypePlan>,
    aliases: &mut BTreeSet<TypeGroupId>,
) -> HeritageTypePlan {
    match ty {
        TSType::TSParenthesizedType(parenthesized) => plan_heritage_type(
            binder,
            declarations,
            scope,
            &parenthesized.type_annotation,
            symbols,
            aliases,
        ),
        TSType::TSIntersectionType(intersection) => {
            let mut terminals = BTreeSet::new();
            let mut absorber = IntersectionAbsorber::None;
            let mut concrete = false;
            let mut opaque = false;
            for member in &intersection.types {
                match plan_heritage_type(binder, declarations, scope, member, symbols, aliases) {
                    HeritageTypePlan::Complete {
                        terminals: member_terminals,
                        absorber: member_absorber,
                    } => {
                        terminals.extend(member_terminals);
                        absorber = absorber.combine(member_absorber);
                        concrete |= member_absorber == IntersectionAbsorber::None;
                    }
                    HeritageTypePlan::Poisoned => return HeritageTypePlan::Poisoned,
                    HeritageTypePlan::Opaque(member_terminals) => {
                        terminals.extend(member_terminals);
                        opaque = true;
                    }
                }
            }
            match absorber {
                IntersectionAbsorber::Never | IntersectionAbsorber::Any => {
                    HeritageTypePlan::Complete {
                        terminals: BTreeSet::new(),
                        absorber,
                    }
                }
                IntersectionAbsorber::None | IntersectionAbsorber::Unknown if opaque => {
                    HeritageTypePlan::Opaque(terminals)
                }
                IntersectionAbsorber::Unknown if concrete => HeritageTypePlan::complete(terminals),
                IntersectionAbsorber::Unknown => {
                    HeritageTypePlan::absorber(IntersectionAbsorber::Unknown)
                }
                IntersectionAbsorber::None => HeritageTypePlan::complete(terminals),
            }
        }
        TSType::TSTypeReference(reference) => {
            if let TSTypeName::IdentifierReference(identifier) = &reference.type_name {
                if let Some(symbol) = symbols.get(identifier.name.as_str()) {
                    return if reference.type_arguments.is_none() {
                        symbol.clone()
                    } else {
                        HeritageTypePlan::Poisoned
                    };
                }
            }
            let group = match topology_type_name_group(binder, scope, &reference.type_name) {
                Ok(group) => group,
                Err(plan) => return plan,
            };
            plan_heritage_group_application(
                binder,
                declarations,
                scope,
                group,
                reference.type_arguments.as_deref(),
                symbols,
                aliases,
            )
        }
        TSType::TSAnyKeyword(_) => HeritageTypePlan::absorber(IntersectionAbsorber::Any),
        TSType::TSNeverKeyword(_) => HeritageTypePlan::absorber(IntersectionAbsorber::Never),
        TSType::TSUnknownKeyword(_) => HeritageTypePlan::absorber(IntersectionAbsorber::Unknown),
        TSType::TSObjectKeyword(_) | TSType::TSTypeLiteral(_) => {
            HeritageTypePlan::complete(BTreeSet::new())
        }
        TSType::TSBigIntKeyword(_)
        | TSType::TSBooleanKeyword(_)
        | TSType::TSIntrinsicKeyword(_)
        | TSType::TSNullKeyword(_)
        | TSType::TSNumberKeyword(_)
        | TSType::TSStringKeyword(_)
        | TSType::TSSymbolKeyword(_)
        | TSType::TSUndefinedKeyword(_)
        | TSType::TSVoidKeyword(_)
        | TSType::TSLiteralType(_) => HeritageTypePlan::Opaque(BTreeSet::new()),
        _ => HeritageTypePlan::Opaque(BTreeSet::new()),
    }
}

fn plan_heritage_group_application(
    binder: &Binder,
    declarations: &TypeDeclTable<'_>,
    scope: ScopeId,
    group: TypeGroupId,
    arguments: Option<&TSTypeParameterInstantiation<'_>>,
    symbols: &BTreeMap<String, HeritageTypePlan>,
    aliases: &mut BTreeSet<TypeGroupId>,
) -> HeritageTypePlan {
    let Some(declaration) = declarations.view(group.index()) else {
        return HeritageTypePlan::Opaque(BTreeSet::new());
    };
    let declaration = match declaration {
        TypeDeclView::Published(published) => {
            let actual_count = arguments.map_or(0, |arguments| arguments.params.len());
            let required_count = published
                .defaults
                .iter()
                .rposition(|default| *default == PublishedTypeParameterDefault::Absent)
                .map_or(0, |index| index + 1);
            return if actual_count >= required_count && actual_count <= published.params.len() {
                HeritageTypePlan::complete(BTreeSet::new())
            } else {
                HeritageTypePlan::Poisoned
            };
        }
        TypeDeclView::Local(declaration) => declaration,
    };
    let (parameter_count, required_count) = match declaration {
        TypeDecl::Interface {
            recovery_params,
            recovery_defaults,
            ..
        } => (
            recovery_params.len(),
            recovery_defaults
                .iter()
                .rposition(|default| *default == PublishedTypeParameterDefault::Absent)
                .map_or(0, |index| index + 1),
        ),
        TypeDecl::Alias {
            params, param_decl, ..
        } => {
            let required = param_decl
                .map(|parameters| {
                    parameters
                        .params
                        .iter()
                        .rposition(|parameter| parameter.default.is_none())
                        .map_or(0, |index| index + 1)
                })
                .unwrap_or(params.len());
            (params.len(), required)
        }
        TypeDecl::Class {
            params,
            param_decl,
            recovery_defaults,
            interfaces,
            ..
        } => {
            let required = if interfaces.is_empty() {
                param_decl
                    .map(|parameters| {
                        parameters
                            .params
                            .iter()
                            .rposition(|parameter| parameter.default.is_none())
                            .map_or(0, |index| index + 1)
                    })
                    .unwrap_or(params.len())
            } else {
                recovery_defaults
                    .iter()
                    .rposition(|default| *default == PublishedTypeParameterDefault::Absent)
                    .map_or(0, |index| index + 1)
            };
            (params.len(), required)
        }
        TypeDecl::Resolved { params, defaults } => (
            params.len(),
            defaults
                .iter()
                .rposition(|default| *default == PublishedTypeParameterDefault::Absent)
                .map_or(0, |index| index + 1),
        ),
        TypeDecl::Unavailable { .. } => (0, 0),
    };
    let actual_count = arguments.map_or(0, |arguments| arguments.params.len());
    if actual_count < required_count || actual_count > parameter_count {
        return HeritageTypePlan::Poisoned;
    }

    match declaration {
        TypeDecl::Interface { .. } => HeritageTypePlan::complete(BTreeSet::from([group])),
        TypeDecl::Alias {
            annotation,
            scope: alias_scope,
            param_decl,
            ..
        } => {
            let mut alias_symbols = BTreeMap::new();
            if let Some(parameters) = param_decl {
                for (index, argument) in arguments
                    .into_iter()
                    .flat_map(|arguments| arguments.params.iter())
                    .enumerate()
                {
                    let Some(parameter) = parameters.params.get(index) else {
                        return HeritageTypePlan::Poisoned;
                    };
                    let argument =
                        plan_heritage_type(binder, declarations, scope, argument, symbols, aliases);
                    alias_symbols.insert(parameter.name.name.to_string(), argument);
                }
            }
            if !aliases.insert(group) {
                return HeritageTypePlan::Poisoned;
            }
            if let Some(parameters) = param_decl {
                for parameter in parameters.params.iter().skip(actual_count) {
                    let argument = parameter
                        .default
                        .as_ref()
                        .map(|default| {
                            plan_heritage_type(
                                binder,
                                declarations,
                                *alias_scope,
                                default,
                                &alias_symbols,
                                aliases,
                            )
                        })
                        .unwrap_or(HeritageTypePlan::Poisoned);
                    alias_symbols.insert(parameter.name.name.to_string(), argument);
                }
            }
            let plan = plan_heritage_type(
                binder,
                declarations,
                *alias_scope,
                annotation,
                &alias_symbols,
                aliases,
            );
            aliases.remove(&group);
            plan
        }
        TypeDecl::Class { .. } => HeritageTypePlan::complete(BTreeSet::from([group])),
        TypeDecl::Unavailable { .. } | TypeDecl::Resolved { .. } => {
            HeritageTypePlan::complete(BTreeSet::new())
        }
    }
}

fn topology_heritage_group(
    binder: &Binder,
    scope: ScopeId,
    heritage: &TSInterfaceHeritage<'_>,
) -> Result<TypeGroupId, HeritageTypePlan> {
    let mut segments = Vec::new();
    if !flatten_heritage_segments(&heritage.expression, &mut segments) {
        return Err(HeritageTypePlan::Opaque(BTreeSet::new()));
    }
    topology_segments_group(binder, scope, &segments)
}

fn topology_type_name_group(
    binder: &Binder,
    scope: ScopeId,
    type_name: &TSTypeName<'_>,
) -> Result<TypeGroupId, HeritageTypePlan> {
    match type_name {
        TSTypeName::IdentifierReference(identifier) => {
            topology_segments_group(binder, scope, &[identifier.name.as_str()])
        }
        TSTypeName::QualifiedName(_) => {
            let mut segments = Vec::new();
            if !flatten_topology_type_name(type_name, &mut segments) {
                return Err(HeritageTypePlan::Opaque(BTreeSet::new()));
            }
            topology_segments_group(binder, scope, &segments)
        }
        TSTypeName::ThisExpression(_) => Err(HeritageTypePlan::Opaque(BTreeSet::new())),
    }
}

fn topology_segments_group(
    binder: &Binder,
    scope: ScopeId,
    segments: &[&str],
) -> Result<TypeGroupId, HeritageTypePlan> {
    match segments {
        ["Array"] => {
            let group = type_decl_id(binder, scope, "Array")
                .ok_or_else(|| HeritageTypePlan::Opaque(BTreeSet::new()))?;
            if type_decl_id(binder, binder.prelude_module, "Array") == Some(group) {
                Err(HeritageTypePlan::Opaque(BTreeSet::new()))
            } else {
                Ok(group)
            }
        }
        [name] => type_decl_id(binder, scope, name).ok_or_else(|| {
            if binder.resolve_type(scope, name).is_some()
                || binder.resolve_value(scope, name).is_some()
            {
                HeritageTypePlan::Opaque(BTreeSet::new())
            } else {
                HeritageTypePlan::Poisoned
            }
        }),
        [_, _, ..] => match binder.resolve_qualified_type_path(scope, segments) {
            crate::binder::namespace::QualifiedTypePathResolution::TypeGroup(group) => Ok(group),
            crate::binder::namespace::QualifiedTypePathResolution::Unavailable { .. }
            | crate::binder::namespace::QualifiedTypePathResolution::Deferred { .. } => {
                Err(HeritageTypePlan::Opaque(BTreeSet::new()))
            }
            _ => Err(HeritageTypePlan::Poisoned),
        },
        [] => Err(HeritageTypePlan::Opaque(BTreeSet::new())),
    }
}

fn flatten_topology_type_name<'name>(
    type_name: &'name TSTypeName<'_>,
    segments: &mut Vec<&'name str>,
) -> bool {
    match type_name {
        TSTypeName::IdentifierReference(identifier) => {
            segments.push(identifier.name.as_str());
            true
        }
        TSTypeName::QualifiedName(qualified) => {
            if !flatten_topology_type_name(&qualified.left, segments) {
                return false;
            }
            segments.push(qualified.right.name.as_str());
            true
        }
        TSTypeName::ThisExpression(_) => false,
    }
}

fn interface_sccs(
    declarations: &TypeDeclTable<'_>,
    candidates: &[usize],
    topology: &InterfaceHeritageTopology,
) -> Vec<Vec<usize>> {
    let nodes: BTreeSet<TypeGroupId> = candidates
        .iter()
        .copied()
        .map(|index| TypeGroupId(u32::try_from(index).expect("type group index fits u32")))
        .collect();
    let graph: BTreeMap<TypeGroupId, BTreeSet<TypeGroupId>> = nodes
        .iter()
        .copied()
        .map(|group| {
            let dependencies = match declarations.get(group.index()) {
                Some(TypeDecl::Interface { fragments, .. }) => fragments
                    .iter()
                    .flat_map(|fragment| {
                        fragment.extends.iter().filter_map(|heritage| {
                            topology
                                .plan(fragment.declaration, heritage)
                                .terminals()
                                .cloned()
                        })
                    })
                    .flatten()
                    .filter(|dependency| nodes.contains(dependency))
                    .collect(),
                _ => BTreeSet::new(),
            };
            (group, dependencies)
        })
        .collect();
    super::classes::construction::dependency_first_sccs(&graph)
        .into_iter()
        .map(|component| component.into_iter().map(TypeGroupId::index).collect())
        .collect()
}

fn class_interface_heritage_sccs(
    binder: &Binder,
    declarations: &TypeDeclTable<'_>,
    topology: &InterfaceHeritageTopology,
) -> Vec<Vec<usize>> {
    let nodes: BTreeSet<TypeGroupId> = declarations
        .iter()
        .enumerate()
        .map(|(index, declaration)| (declarations.published_len() + index, declaration))
        .filter(|(_, declaration)| matches!(declaration, TypeDecl::Class { .. }))
        .map(|(index, _)| TypeGroupId(u32::try_from(index).expect("type group index fits u32")))
        .collect();
    let graph: BTreeMap<TypeGroupId, BTreeSet<TypeGroupId>> = nodes
        .iter()
        .copied()
        .map(|group| {
            let dependencies = match declarations.get(group.index()) {
                Some(TypeDecl::Class { interfaces, .. }) => {
                    let mut dependencies = interfaces
                        .iter()
                        .flat_map(|fragment| {
                            fragment.extends.iter().filter_map(|heritage| {
                                topology
                                    .plan(fragment.declaration, heritage)
                                    .terminals()
                                    .cloned()
                            })
                        })
                        .flatten()
                        .filter(|dependency| nodes.contains(dependency))
                        .collect::<BTreeSet<_>>();
                    dependencies.extend(
                        class_heritage_topology_terminals(binder, declarations, group)
                            .into_iter()
                            .flatten()
                            .filter(|dependency| nodes.contains(dependency)),
                    );
                    dependencies
                }
                _ => BTreeSet::new(),
            };
            (group, dependencies)
        })
        .collect();
    super::classes::construction::dependency_first_sccs(&graph)
        .into_iter()
        .map(|component| component.into_iter().map(TypeGroupId::index).collect())
        .collect()
}

fn class_heritage_topology_terminals(
    binder: &Binder,
    declarations: &TypeDeclTable<'_>,
    group: TypeGroupId,
) -> Option<BTreeSet<TypeGroupId>> {
    let TypeDecl::Class {
        scope,
        class,
        param_decl,
        ..
    } = declarations.get(group.index())?
    else {
        return None;
    };
    let heritage = class.super_class.as_ref()?;
    let mut segments = Vec::new();
    if !flatten_heritage_segments(heritage, &mut segments) {
        return None;
    }
    let target = topology_segments_group(binder, *scope, &segments).ok()?;
    let symbols = param_decl
        .iter()
        .flat_map(|parameters| parameters.params.iter())
        .map(|parameter| {
            (
                parameter.name.name.to_string(),
                HeritageTypePlan::absorber(IntersectionAbsorber::Unknown),
            )
        })
        .collect();
    plan_heritage_group_application(
        binder,
        declarations,
        *scope,
        target,
        class.super_type_arguments.as_deref(),
        &symbols,
        &mut BTreeSet::new(),
    )
    .into_topology_plan()
    .terminals()
    .cloned()
}

fn class_interface_component_has_cycle(
    declarations: &TypeDeclTable<'_>,
    component: &[usize],
    topology: &InterfaceHeritageTopology,
) -> bool {
    if component.len() > 1 {
        return true;
    }
    let Some(&index) = component.first() else {
        return false;
    };
    let group = TypeGroupId(u32::try_from(index).expect("type group index fits u32"));
    let Some(TypeDecl::Class { interfaces, .. }) = declarations.get(index) else {
        return false;
    };
    interfaces.iter().any(|fragment| {
        fragment.extends.iter().any(|heritage| {
            topology
                .plan(fragment.declaration, heritage)
                .terminals()
                .is_some_and(|terminals| terminals.contains(&group))
        })
    })
}

fn class_interface_component_has_soft_edge(
    declarations: &TypeDeclTable<'_>,
    component: &[usize],
    topology: &InterfaceHeritageTopology,
) -> bool {
    let members: BTreeSet<usize> = component.iter().copied().collect();
    component.iter().copied().any(|index| {
        let Some(TypeDecl::Class { interfaces, .. }) = declarations.get(index) else {
            return false;
        };
        interfaces.iter().any(|fragment| {
            fragment.extends.iter().any(|heritage| {
                topology
                    .plan(fragment.declaration, heritage)
                    .terminals()
                    .is_some_and(|terminals| {
                        terminals
                            .iter()
                            .any(|target| members.contains(&target.index()))
                    })
            })
        })
    })
}

fn interface_component_has_cycle(
    declarations: &TypeDeclTable<'_>,
    component: &[usize],
    topology: &InterfaceHeritageTopology,
) -> bool {
    if component.len() > 1 {
        return true;
    }
    let Some(&index) = component.first() else {
        return false;
    };
    let group = TypeGroupId(u32::try_from(index).expect("type group index fits u32"));
    let Some(TypeDecl::Interface { fragments, .. }) = declarations.get(index) else {
        return false;
    };
    fragments.iter().any(|fragment| {
        fragment.extends.iter().any(|heritage| {
            topology
                .plan(fragment.declaration, heritage)
                .terminals()
                .is_some_and(|terminals| terminals.contains(&group))
        })
    })
}

fn heritage_display_name(heritage: &TSInterfaceHeritage<'_>) -> String {
    let mut segments = Vec::new();
    if flatten_heritage_segments(&heritage.expression, &mut segments) {
        segments.join(".")
    } else {
        "<heritage>".to_string()
    }
}

fn flatten_heritage_segments<'a>(
    expression: &'a Expression<'_>,
    segments: &mut Vec<&'a str>,
) -> bool {
    match expression {
        Expression::Identifier(identifier) => {
            segments.push(identifier.name.as_str());
            true
        }
        Expression::StaticMemberExpression(member) => {
            if !flatten_heritage_segments(&member.object, segments) {
                return false;
            }
            segments.push(member.property.name.as_str());
            true
        }
        _ => false,
    }
}

/// Reserve top-level type declarations by [`TypeGroupId`].
/// Interfaces get ids before bodies resolve, enabling self/sibling references.
/// Reserve runs per compilation unit; append order matches legacy storage order so
/// prelude and user declarations stay index-aligned.
#[allow(clippy::too_many_arguments)] // Two counters + two appended tables — irreducible reserve state.
pub(in crate::check::checker) fn reserve_type_decls<'ast>(
    interner: &mut Interner,
    binder: &Binder,
    module: ScopeId,
    program: &'ast Program<'ast>,
    next_type_param: &mut u32,
    next_class_id: &mut u32,
    decls: &mut TypeDeclTable<'ast>,
    resolved: &mut TypeResolvedTable,
) {
    let state = TypeDeclReservationState {
        interner,
        binder,
        next_type_param,
        next_class_id,
        decls,
        resolved,
    };
    reserve_type_decls_with_selection(state, module, program, None, false, None);
}

pub(in crate::check::checker) struct TypeDeclReservationState<'a, 'ast> {
    pub interner: &'a mut Interner,
    pub binder: &'a Binder,
    pub next_type_param: &'a mut u32,
    pub next_class_id: &'a mut u32,
    pub decls: &'a mut TypeDeclTable<'ast>,
    pub resolved: &'a mut TypeResolvedTable,
}

pub(in crate::check::checker) fn reserve_type_decls_for_combined_library<'ast>(
    state: TypeDeclReservationState<'_, 'ast>,
    module: ScopeId,
    program: &'ast Program<'ast>,
) {
    reserve_type_decls_with_selection(state, module, program, None, true, None);
}

pub(in crate::check::checker) fn reserve_type_decls_for_combined_user<'ast>(
    state: TypeDeclReservationState<'_, 'ast>,
    module: ScopeId,
    program: &'ast Program<'ast>,
    library_declarations: &FxHashSet<DeclId>,
) {
    reserve_type_decls_with_selection(
        state,
        module,
        program,
        None,
        false,
        Some(library_declarations),
    );
}

pub(in crate::check::checker) fn reserve_type_decls_selected<'ast>(
    state: TypeDeclReservationState<'_, 'ast>,
    module: ScopeId,
    program: &'ast Program<'ast>,
    selected: &BTreeSet<ReplayOwner>,
) {
    reserve_type_decls_with_selection(state, module, program, Some(selected), false, None);
}

fn reserve_type_decls_with_selection<'ast>(
    state: TypeDeclReservationState<'_, 'ast>,
    module: ScopeId,
    program: &'ast Program<'ast>,
    selected: Option<&BTreeSet<ReplayOwner>>,
    retain_library_declarations: bool,
    preserved_library_declarations: Option<&FxHashSet<DeclId>>,
) {
    let TypeDeclReservationState {
        interner,
        binder,
        next_type_param,
        next_class_id,
        decls,
        resolved,
    } = state;
    // The AST walk is joined to the binder through the exact lexical declaration,
    // never by selecting a first/last declaration from a same-name group.
    walk_type_decls(
        binder,
        module,
        program,
        &mut |_walk_scope, _, declaration| {
            if let Some(selected) = selected {
                let (binding_start, kind) = match declaration {
                    TopTypeDecl::Interface(interface) => {
                        (interface.id.span.start, BinderDeclarationKind::Interface)
                    }
                    TopTypeDecl::Alias(alias) => {
                        (alias.id.span.start, BinderDeclarationKind::TypeAlias)
                    }
                    TopTypeDecl::Class(class) => {
                        (class_binding_start(class), BinderDeclarationKind::Class)
                    }
                };
                let Some(exact) = binder.exact_declaration_at(module, binding_start, kind) else {
                    return;
                };
                let group_selected = exact
                    .type_group
                    .is_some_and(|group| selected.contains(&ReplayOwner::TypeGroup(group)));
                let class_selected = exact
                    .type_group
                    .and_then(|group| decls.get(group.index()))
                    .and_then(|decl| match decl {
                        TypeDecl::Class { class_id, .. } => Some(*class_id),
                        _ => None,
                    })
                    .is_some_and(|class| selected.contains(&ReplayOwner::Class(class)));
                if !group_selected && !class_selected {
                    return;
                }
            }
            match declaration {
                TopTypeDecl::Interface(iface) => {
                    let Some(exact) = binder.exact_declaration_at(
                        module,
                        iface.id.span.start,
                        BinderDeclarationKind::Interface,
                    ) else {
                        return;
                    };
                    let (Some(group), Some(scope)) = (exact.type_group, exact.site.scope) else {
                        return;
                    };
                    let declaration = exact.id;
                    if selected.is_none()
                        && !retain_library_declarations
                        && binder
                            .namespaces
                            .merge_disposition_for_declaration(declaration)
                            .is_some_and(|disposition| disposition != MergeDisposition::Admitted)
                    {
                        let _ =
                            alloc_type_param_ids(iface.type_parameters.as_deref(), next_type_param);
                        if group.index() < decls.published_len()
                            && decls.has_replacement(group.index())
                        {
                            return;
                        }
                        ensure_type_group_slot(decls, group.index());
                        terminalize_displaced_type_draft(interner, &decls[group.index()]);
                        decls[group.index()] = TypeDecl::Unavailable { declaration };
                        if let Some(slot) = resolved.get_mut(group.index()) {
                            *slot = None;
                        }
                        return;
                    }
                    let mut fragment = InterfaceFragment {
                        declaration,
                        scope,
                        param_decl: iface.type_parameters.as_deref(),
                        params: Vec::new(),
                        members: &iface.body.body,
                        extends: &iface.extends,
                    };
                    ensure_type_group_slot(decls, group.index());
                    match decls.get_mut(group.index()) {
                        Some(TypeDecl::Interface {
                            recovery_params,
                            recovery_names,
                            recovery_defaults,
                            param_slots,
                            fragments,
                            ..
                        }) => {
                            fragment.params = recover_interface_fragment_params(
                                param_slots,
                                recovery_params,
                                recovery_names,
                                recovery_defaults,
                                fragment.param_decl,
                                next_type_param,
                            );
                            fragments.push(fragment);
                            sort_interface_fragments(binder, group, fragments);
                        }
                        Some(TypeDecl::Class {
                            params,
                            recovery_names,
                            recovery_defaults,
                            param_slots,
                            interfaces,
                            header_fragments,
                            ..
                        }) => {
                            fragment.params = recover_interface_fragment_params(
                                param_slots,
                                params,
                                recovery_names,
                                recovery_defaults,
                                fragment.param_decl,
                                next_type_param,
                            );
                            header_fragments.push(header_fragment_binding(
                                declaration,
                                fragment.param_decl,
                                &fragment.params,
                            ));
                            sort_header_fragment_bindings(binder, group, header_fragments);
                            interfaces.push(fragment);
                            sort_interface_fragments(binder, group, interfaces);
                        }
                        Some(TypeDecl::Resolved {
                            defaults: inherited_defaults,
                            ..
                        }) => {
                            let inherited_defaults = inherited_defaults.clone();
                            let reserved = interner.reserve_object();
                            if let Some(slot) = resolved.get_mut(group.index()) {
                                *slot = Some(reserved);
                            }
                            let params = alloc_type_param_ids(
                                iface.type_parameters.as_deref(),
                                next_type_param,
                            );
                            let defaults = vec![None; params.len()];
                            fragment.params = params.clone();
                            let param_slots = iface
                                .type_parameters
                                .as_deref()
                                .into_iter()
                                .flat_map(|declaration| declaration.params.iter())
                                .enumerate()
                                .zip(params.iter().copied())
                                .map(|((index, parameter), id)| {
                                    ((index, parameter.name.name.to_string()), id)
                                })
                                .collect();
                            let recovery_names = iface
                                .type_parameters
                                .as_deref()
                                .into_iter()
                                .flat_map(|declaration| declaration.params.iter())
                                .map(|parameter| parameter.name.name.to_string())
                                .collect();
                            decls[group.index()] = TypeDecl::Interface {
                                declaration,
                                scope,
                                reserved,
                                params,
                                recovery_params: fragment.params.clone(),
                                recovery_names,
                                recovery_defaults: if inherited_defaults.len()
                                    == fragment.params.len()
                                {
                                    inherited_defaults
                                } else {
                                    vec![
                                        PublishedTypeParameterDefault::Absent;
                                        fragment.params.len()
                                    ]
                                },
                                param_slots,
                                conflict_alternatives: Vec::new(),
                                defaults,
                                parameter_descriptors: None,
                                param_decl: iface.type_parameters.as_deref(),
                                extends: &iface.extends,
                                fragments: vec![fragment],
                            };
                        }
                        Some(displaced @ TypeDecl::Alias { .. })
                        | Some(displaced @ TypeDecl::Unavailable { .. }) => {
                            terminalize_displaced_type_draft(interner, displaced);
                            *displaced = TypeDecl::Unavailable { declaration };
                            if let Some(slot) = resolved.get_mut(group.index()) {
                                *slot = None;
                            }
                        }
                        None => unreachable!("type group slot was extended"),
                    }
                }
                TopTypeDecl::Alias(alias) => {
                    let exact = binder.exact_declaration_at(
                        module,
                        alias.id.span.start,
                        BinderDeclarationKind::TypeAlias,
                    );
                    let (group, declaration, scope) = match exact {
                        Some(exact) => match (exact.type_group, exact.site.scope) {
                            (Some(group), Some(scope)) => (Some(group), Some(exact.id), scope),
                            _ => (None, None, _walk_scope),
                        },
                        None => (None, None, _walk_scope),
                    };
                    let params =
                        alloc_type_param_ids(alias.type_parameters.as_deref(), next_type_param);
                    let defaults = vec![None; params.len()];
                    if let (Some(group), Some(declaration)) = (group, declaration) {
                        if selected.is_none()
                            && !retain_library_declarations
                            && binder
                                .namespaces
                                .merge_disposition_for_declaration(declaration)
                                .is_some_and(|disposition| {
                                    disposition != MergeDisposition::Admitted
                                })
                        {
                            let preserves_library_alias = preserved_library_declarations
                                .is_some_and(|library_declarations| {
                                    matches!(
                                        decls.get(group.index()),
                                        Some(TypeDecl::Alias { declaration, .. })
                                            if library_declarations.contains(declaration)
                                    )
                                });
                            if preserves_library_alias {
                                return;
                            }
                            if group.index() < decls.published_len()
                                && decls.has_replacement(group.index())
                            {
                                return;
                            }
                            ensure_type_group_slot(decls, group.index());
                            terminalize_displaced_type_draft(interner, &decls[group.index()]);
                            decls[group.index()] = TypeDecl::Unavailable { declaration };
                            if let Some(slot) = resolved.get_mut(group.index()) {
                                *slot = None;
                            }
                            return;
                        }
                        ensure_type_group_slot(decls, group.index());
                        if !matches!(decls.get(group.index()), Some(TypeDecl::Resolved { .. })) {
                            terminalize_displaced_type_draft(interner, &decls[group.index()]);
                            decls[group.index()] = TypeDecl::Unavailable { declaration };
                            if let Some(slot) = resolved.get_mut(group.index()) {
                                *slot = None;
                            }
                        } else {
                            // A private replacement must not inherit the published alias memo.
                            if let Some(slot) = resolved.get_mut(group.index()) {
                                *slot = None;
                            }
                            // Reserve recursive templates only after the declaration wins its
                            // group slot; rejected aliases never publish a draft identity.
                            let conditional_template = if matches!(
                                alias.type_annotation,
                                oxc_ast::ast::TSType::TSConditionalType(_)
                            ) {
                                let reserved = interner.reserve_conditional();
                                interner.set_template_name(reserved, alias.id.name.as_str());
                                if let Some(slot) = resolved.get_mut(group.index()) {
                                    *slot = Some(reserved);
                                }
                                Some(reserved)
                            } else {
                                None
                            };
                            let mapped_template = if matches!(
                                alias.type_annotation,
                                oxc_ast::ast::TSType::TSMappedType(_)
                            ) {
                                let reserved = interner.reserve_mapped();
                                interner.set_template_name(reserved, alias.id.name.as_str());
                                if let Some(slot) = resolved.get_mut(group.index()) {
                                    *slot = Some(reserved);
                                }
                                Some(reserved)
                            } else {
                                None
                            };
                            let object_template = if alias.type_parameters.is_none()
                                && matches!(
                                    alias.type_annotation,
                                    oxc_ast::ast::TSType::TSTypeLiteral(_)
                                ) {
                                let reserved = interner.reserve_object();
                                if let Some(slot) = resolved.get_mut(group.index()) {
                                    *slot = Some(reserved);
                                }
                                Some(reserved)
                            } else {
                                None
                            };
                            decls[group.index()] = TypeDecl::Alias {
                                declaration,
                                scope,
                                annotation: &alias.type_annotation,
                                params,
                                defaults,
                                param_decl: alias.type_parameters.as_deref(),
                                resolving: false,
                                conditional_template,
                                mapped_template,
                                object_template,
                                name: alias.id.name.to_string(),
                                name_span: Span::from_oxc(alias.id.span),
                            };
                        }
                    }
                }
                // Named classes reserve only a stable nominal identity. Their immutable
                // instance/static templates are constructed by class publication.
                TopTypeDecl::Class(class) => {
                    // M13: a fresh stable `ClassId` for this declaration (source order),
                    // stamped onto its members during class publication.
                    let class_id = reserve_class_id(next_class_id);
                    {
                        let exact = binder.exact_declaration_at(
                            module,
                            class_binding_start(class),
                            BinderDeclarationKind::Class,
                        );
                        let (group, declaration, scope) = match exact {
                            Some(exact) => match (exact.type_group, exact.site.scope) {
                                (Some(group), Some(scope)) => (Some(group), Some(exact.id), scope),
                                _ => (None, None, _walk_scope),
                            },
                            None => (None, None, _walk_scope),
                        };
                        if let (Some(group), Some(declaration)) = (group, declaration) {
                            let rejected_global_merge = selected.is_none()
                                && !retain_library_declarations
                                && binder
                                    .namespaces
                                    .merge_disposition_for_declaration(declaration)
                                    .is_some_and(|disposition| {
                                        disposition != MergeDisposition::Admitted
                                    });
                            if rejected_global_merge {
                                let _ = alloc_type_param_ids(
                                    class.type_parameters.as_deref(),
                                    next_type_param,
                                );
                                return;
                            }
                            ensure_type_group_slot(decls, group.index());
                            match decls.get(group.index()) {
                                Some(TypeDecl::Interface {
                                    reserved,
                                    recovery_params,
                                    recovery_names,
                                    recovery_defaults,
                                    param_slots,
                                    conflict_alternatives,
                                    parameter_descriptors,
                                    fragments,
                                    ..
                                }) => {
                                    let mut header_fragments = fragments
                                        .iter()
                                        .map(|fragment| {
                                            header_fragment_binding(
                                                fragment.declaration,
                                                fragment.param_decl,
                                                &fragment.params,
                                            )
                                        })
                                        .collect::<Vec<_>>();
                                    let mut params = recovery_params.clone();
                                    let mut recovery_names = recovery_names.clone();
                                    let mut recovery_defaults = recovery_defaults.clone();
                                    let mut param_slots = param_slots.clone();
                                    let class_params = recover_interface_fragment_params(
                                        &mut param_slots,
                                        &mut params,
                                        &mut recovery_names,
                                        &mut recovery_defaults,
                                        class.type_parameters.as_deref(),
                                        next_type_param,
                                    );
                                    let class_header = header_fragment_binding(
                                        declaration,
                                        class.type_parameters.as_deref(),
                                        &class_params,
                                    );
                                    header_fragments.push(class_header);
                                    sort_header_fragment_bindings(
                                        binder,
                                        group,
                                        &mut header_fragments,
                                    );
                                    interner.abandon_reserved_object(*reserved).expect(
                                        "class merge displaces one pending interface reservation",
                                    );
                                    decls[group.index()] = TypeDecl::Class {
                                        declaration,
                                        scope,
                                        class_id,
                                        params,
                                        class_params,
                                        recovery_names,
                                        recovery_defaults,
                                        param_slots,
                                        conflict_alternatives: conflict_alternatives.clone(),
                                        parameter_descriptors: parameter_descriptors.clone(),
                                        param_decl: class.type_parameters.as_deref(),
                                        class,
                                        interfaces: fragments.clone(),
                                        header_fragments,
                                    };
                                }
                                Some(TypeDecl::Resolved {
                                    defaults: inherited_defaults,
                                    ..
                                }) => {
                                    let inherited_defaults = inherited_defaults.clone();
                                    // M16: allocate one id per declared type parameter (in source
                                    // order), paired with names when the class body is lowered.
                                    let params = alloc_type_param_ids(
                                        class.type_parameters.as_deref(),
                                        next_type_param,
                                    );
                                    let param_slots = class
                                        .type_parameters
                                        .as_deref()
                                        .into_iter()
                                        .flat_map(|declaration| declaration.params.iter())
                                        .enumerate()
                                        .zip(params.iter().copied())
                                        .map(|((index, parameter), id)| {
                                            ((index, parameter.name.name.to_string()), id)
                                        })
                                        .collect();
                                    let recovery_names = class
                                        .type_parameters
                                        .as_deref()
                                        .into_iter()
                                        .flat_map(|declaration| declaration.params.iter())
                                        .map(|parameter| parameter.name.name.to_string())
                                        .collect();
                                    decls[group.index()] = TypeDecl::Class {
                                        declaration,
                                        scope,
                                        class_id,
                                        class_params: params.clone(),
                                        recovery_defaults: if inherited_defaults.len()
                                            == params.len()
                                        {
                                            inherited_defaults
                                        } else {
                                            vec![
                                                PublishedTypeParameterDefault::Absent;
                                                params.len()
                                            ]
                                        },
                                        recovery_names,
                                        param_slots,
                                        conflict_alternatives: Vec::new(),
                                        parameter_descriptors: None,
                                        header_fragments: vec![header_fragment_binding(
                                            declaration,
                                            class.type_parameters.as_deref(),
                                            &params,
                                        )],
                                        interfaces: Vec::new(),
                                        params,
                                        param_decl: class.type_parameters.as_deref(),
                                        class,
                                    };
                                }
                                Some(TypeDecl::Class { .. })
                                    if group.index() < decls.published_len() =>
                                {
                                    // A private collision rebuild has already restored the
                                    // library class winner for this frozen-prefix group.
                                    let _ = alloc_type_param_ids(
                                        class.type_parameters.as_deref(),
                                        next_type_param,
                                    );
                                }
                                Some(_) => {
                                    // Preserve reservation monotonicity for rejected duplicate
                                    // class/type compositions even though their surface is absent.
                                    let _ = alloc_type_param_ids(
                                        class.type_parameters.as_deref(),
                                        next_type_param,
                                    );
                                    terminalize_displaced_type_draft(
                                        interner,
                                        &decls[group.index()],
                                    );
                                    decls[group.index()] = TypeDecl::Unavailable { declaration };
                                    if let Some(slot) = resolved.get_mut(group.index()) {
                                        *slot = None;
                                    }
                                }
                                None => unreachable!("type group slot was extended"),
                            }
                        }
                    }
                }
            }
        },
    );
}

fn ensure_type_group_slot<'ast>(decls: &mut TypeDeclTable<'ast>, index: usize) {
    while decls.len() < index {
        decls.push(TypeDecl::Resolved {
            params: Vec::new(),
            defaults: Vec::new(),
        });
    }
    if decls.len() == index {
        decls.push(TypeDecl::Resolved {
            params: Vec::new(),
            defaults: Vec::new(),
        });
    }
    let _ = decls.get_mut(index);
}

fn terminalize_displaced_type_draft(interner: &mut Interner, declaration: &TypeDecl<'_>) {
    match declaration {
        TypeDecl::Interface { reserved, .. } => interner
            .abandon_reserved_object(*reserved)
            .expect("displaced interface owns one pending object reservation"),
        TypeDecl::Alias {
            conditional_template,
            mapped_template,
            object_template,
            ..
        } => {
            if let Some(reserved) = conditional_template {
                interner
                    .poison_reserved_conditional(*reserved)
                    .expect("displaced conditional alias owns one pending reservation");
            }
            if let Some(reserved) = mapped_template {
                interner
                    .poison_reserved_mapped(*reserved)
                    .expect("displaced mapped alias owns one pending reservation");
            }
            if let Some(reserved) = object_template {
                interner
                    .abandon_reserved_object(*reserved)
                    .expect("displaced object alias owns one pending reservation");
            }
        }
        TypeDecl::Class { .. } | TypeDecl::Resolved { .. } | TypeDecl::Unavailable { .. } => {}
    }
}

fn sort_interface_fragments(
    binder: &Binder,
    group: TypeGroupId,
    fragments: &mut [InterfaceFragment<'_>],
) {
    let Some(bound) = binder.type_groups.get(group) else {
        return;
    };
    fragments.sort_by_key(|fragment| {
        bound
            .fragments
            .iter()
            .position(|candidate| candidate.declaration == fragment.declaration)
            .unwrap_or(usize::MAX)
    });
}

#[derive(Copy, Clone)]
pub(in crate::check::checker) enum TopTypeDecl<'ast> {
    Interface(&'ast TSInterfaceDeclaration<'ast>),
    Alias(&'ast TSTypeAliasDeclaration<'ast>),
    Class(&'ast Class<'ast>),
}

/// Visit every named type declaration with the exact lexical scope allocated by
/// the binder. The walk mirrors binder scope entry and never creates a scope.
pub(in crate::check::checker) fn walk_type_decls<'ast>(
    binder: &Binder,
    module: ScopeId,
    program: &'ast Program<'ast>,
    visit: &mut impl FnMut(ScopeId, u32, TopTypeDecl<'ast>),
) {
    let lexical_root = binder.statement_lexical_root(module);
    walk_type_decl_statements(binder, module, lexical_root, &program.body, visit);
}

fn walk_type_decl_statements<'ast>(
    binder: &Binder,
    module: ScopeId,
    scope: ScopeId,
    statements: &'ast [Statement<'ast>],
    visit: &mut impl FnMut(ScopeId, u32, TopTypeDecl<'ast>),
) {
    for statement in statements {
        walk_type_decl_statement(binder, module, scope, statement, visit);
    }
}

fn walk_type_decl_statement<'ast>(
    binder: &Binder,
    module: ScopeId,
    scope: ScopeId,
    statement: &'ast Statement<'ast>,
    visit: &mut impl FnMut(ScopeId, u32, TopTypeDecl<'ast>),
) {
    match statement {
        Statement::TSInterfaceDeclaration(interface) => {
            visit_bound_type(
                binder,
                module,
                scope,
                interface.span.start,
                BinderDeclarationKind::Interface,
                interface.id.span.start,
                TopTypeDecl::Interface(interface),
                visit,
            );
        }
        Statement::TSTypeAliasDeclaration(alias) => {
            visit_bound_type(
                binder,
                module,
                scope,
                alias.span.start,
                BinderDeclarationKind::TypeAlias,
                alias.id.span.start,
                TopTypeDecl::Alias(alias),
                visit,
            );
        }
        Statement::ClassDeclaration(class) => {
            if let Some(id) = &class.id {
                visit_bound_type(
                    binder,
                    module,
                    scope,
                    class.span.start,
                    BinderDeclarationKind::Class,
                    id.span.start,
                    TopTypeDecl::Class(class),
                    visit,
                );
            }
            walk_type_decl_class(binder, module, scope, class, visit);
        }
        Statement::TSModuleDeclaration(namespace) => {
            walk_type_decl_namespace(binder, module, scope, namespace, visit)
        }
        Statement::TSGlobalDeclaration(global) => {
            let global_scope = binder
                .global_augmentation_body_scope(module, global.global_span.start)
                .unwrap_or(scope);
            walk_type_decl_statements(binder, module, global_scope, &global.body.body, visit)
        }
        Statement::FunctionDeclaration(function) => {
            walk_type_decl_function(binder, module, function, visit);
        }
        Statement::ExportNamedDeclaration(export) => {
            let Some(declaration) = &export.declaration else {
                return;
            };
            match declaration {
                Declaration::TSInterfaceDeclaration(interface) => visit_bound_type(
                    binder,
                    module,
                    scope,
                    export.span.start,
                    BinderDeclarationKind::Interface,
                    interface.id.span.start,
                    TopTypeDecl::Interface(interface),
                    visit,
                ),
                Declaration::TSTypeAliasDeclaration(alias) => {
                    visit_bound_type(
                        binder,
                        module,
                        scope,
                        export.span.start,
                        BinderDeclarationKind::TypeAlias,
                        alias.id.span.start,
                        TopTypeDecl::Alias(alias),
                        visit,
                    );
                }
                Declaration::ClassDeclaration(class) => {
                    if let Some(id) = &class.id {
                        visit_bound_type(
                            binder,
                            module,
                            scope,
                            export.span.start,
                            BinderDeclarationKind::Class,
                            id.span.start,
                            TopTypeDecl::Class(class),
                            visit,
                        );
                    }
                    walk_type_decl_class(binder, module, scope, class, visit);
                }
                Declaration::TSModuleDeclaration(namespace) => {
                    walk_type_decl_namespace(binder, module, scope, namespace, visit)
                }
                Declaration::TSGlobalDeclaration(global) => {
                    let global_scope = binder
                        .global_augmentation_body_scope(module, global.global_span.start)
                        .unwrap_or(scope);
                    walk_type_decl_statements(
                        binder,
                        module,
                        global_scope,
                        &global.body.body,
                        visit,
                    )
                }
                Declaration::FunctionDeclaration(function) => {
                    walk_type_decl_function(binder, module, function, visit);
                }
                Declaration::VariableDeclaration(declaration) => {
                    walk_type_decl_variable(binder, module, scope, declaration, visit);
                }
                _ => {}
            }
        }
        Statement::ExportDefaultDeclaration(export) => match &export.declaration {
            ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                if binder
                    .exact_declaration_at(
                        module,
                        class_binding_start(class),
                        BinderDeclarationKind::Class,
                    )
                    .is_none()
                {
                    return;
                }
                visit_bound_type(
                    binder,
                    module,
                    scope,
                    export.span.start,
                    BinderDeclarationKind::Class,
                    class_binding_start(class),
                    TopTypeDecl::Class(class),
                    visit,
                );
                walk_type_decl_class(binder, module, scope, class, visit);
            }
            ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                if !binder
                    .fn_decl_ids
                    .contains_key(&(module, function.span.start))
                {
                    return;
                }
                walk_type_decl_function(binder, module, function, visit);
            }
            declaration => {
                if let Some(expression) = declaration.as_expression() {
                    if binder
                        .default_export_value(module, export.span.start)
                        .is_none()
                    {
                        return;
                    }
                    walk_type_decl_expression(binder, module, scope, expression, visit);
                }
            }
        },
        Statement::VariableDeclaration(declaration) => {
            walk_type_decl_variable(binder, module, scope, declaration, visit);
        }
        Statement::ExpressionStatement(statement) => {
            walk_type_decl_expression(binder, module, scope, &statement.expression, visit);
        }
        Statement::ReturnStatement(statement) => {
            if let Some(argument) = &statement.argument {
                walk_type_decl_expression(binder, module, scope, argument, visit);
            }
        }
        Statement::ThrowStatement(statement) => {
            walk_type_decl_expression(binder, module, scope, &statement.argument, visit);
        }
        Statement::IfStatement(statement) => {
            walk_type_decl_expression(binder, module, scope, &statement.test, visit);
            walk_type_decl_statement(binder, module, scope, &statement.consequent, visit);
            if let Some(alternate) = &statement.alternate {
                walk_type_decl_statement(binder, module, scope, alternate, visit);
            }
        }
        Statement::BlockStatement(block) => {
            walk_type_decl_block(binder, module, scope, block, visit);
        }
        Statement::SwitchStatement(statement) => {
            walk_type_decl_expression(binder, module, scope, &statement.discriminant, visit);
            let Some(&switch_scope) = binder.block_scopes.get(&(module, statement.span.start))
            else {
                return;
            };
            for case in &statement.cases {
                if let Some(test) = &case.test {
                    walk_type_decl_expression(binder, module, switch_scope, test, visit);
                }
                walk_type_decl_statements(binder, module, switch_scope, &case.consequent, visit);
            }
        }
        Statement::WhileStatement(statement) => {
            walk_type_decl_expression(binder, module, scope, &statement.test, visit);
            walk_type_decl_statement(binder, module, scope, &statement.body, visit);
        }
        Statement::DoWhileStatement(statement) => {
            walk_type_decl_statement(binder, module, scope, &statement.body, visit);
            walk_type_decl_expression(binder, module, scope, &statement.test, visit);
        }
        Statement::ForStatement(statement) => {
            let Some(&loop_scope) = binder.block_scopes.get(&(module, statement.span.start)) else {
                return;
            };
            if let Some(init) = &statement.init {
                match init {
                    ForStatementInit::VariableDeclaration(declaration) => {
                        walk_type_decl_variable(binder, module, loop_scope, declaration, visit);
                    }
                    other => {
                        if let Some(expression) = other.as_expression() {
                            walk_type_decl_expression(
                                binder, module, loop_scope, expression, visit,
                            );
                        }
                    }
                }
            }
            if let Some(test) = &statement.test {
                walk_type_decl_expression(binder, module, loop_scope, test, visit);
            }
            if let Some(update) = &statement.update {
                walk_type_decl_expression(binder, module, loop_scope, update, visit);
            }
            walk_type_decl_statement(binder, module, loop_scope, &statement.body, visit);
        }
        Statement::ForInStatement(statement) => {
            walk_type_decl_for_in_of(
                binder,
                module,
                scope,
                &statement.left,
                &statement.right,
                &statement.body,
                statement.span.start,
                visit,
            );
        }
        Statement::ForOfStatement(statement) => {
            walk_type_decl_for_in_of(
                binder,
                module,
                scope,
                &statement.left,
                &statement.right,
                &statement.body,
                statement.span.start,
                visit,
            );
        }
        Statement::LabeledStatement(statement) => {
            walk_type_decl_statement(binder, module, scope, &statement.body, visit);
        }
        Statement::TryStatement(statement) => {
            walk_type_decl_block(binder, module, scope, &statement.block, visit);
            if let Some(handler) = &statement.handler {
                let Some(&catch_scope) = binder.block_scopes.get(&(module, handler.span.start))
                else {
                    return;
                };
                walk_type_decl_block(binder, module, catch_scope, &handler.body, visit);
            }
            if let Some(finalizer) = &statement.finalizer {
                walk_type_decl_block(binder, module, scope, finalizer, visit);
            }
        }
        _ => {}
    }
}

pub(in crate::check::checker) fn class_binding_start(class: &Class<'_>) -> u32 {
    class
        .id
        .as_ref()
        .map_or(class.span.start, |identifier| identifier.span.start)
}

#[allow(clippy::too_many_arguments)]
fn visit_bound_type<'ast>(
    binder: &Binder,
    module: ScopeId,
    fallback_scope: ScopeId,
    owner_start: u32,
    kind: BinderDeclarationKind,
    binding_start: u32,
    declaration: TopTypeDecl<'ast>,
    visit: &mut impl FnMut(ScopeId, u32, TopTypeDecl<'ast>),
) {
    let matched = binder
        .exact_declaration_at(module, binding_start, kind)
        .and_then(|declaration| declaration.site.scope);
    let scope = matched.unwrap_or(fallback_scope);
    visit(scope, owner_start, declaration);
}

fn walk_type_decl_namespace<'ast>(
    binder: &Binder,
    module: ScopeId,
    scope: ScopeId,
    declaration: &'ast TSModuleDeclaration<'ast>,
    visit: &mut impl FnMut(ScopeId, u32, TopTypeDecl<'ast>),
) {
    let scope = binder
        .namespace_fragment_private_scope(module, declaration.span.start)
        .unwrap_or(scope);
    match &declaration.body {
        Some(TSModuleDeclarationBody::TSModuleBlock(block)) => {
            walk_type_decl_statements(binder, module, scope, &block.body, visit);
        }
        Some(TSModuleDeclarationBody::TSModuleDeclaration(nested)) => {
            walk_type_decl_namespace(binder, module, scope, nested, visit);
        }
        None => {}
    }
}

fn walk_type_decl_block<'ast>(
    binder: &Binder,
    module: ScopeId,
    parent: ScopeId,
    block: &'ast oxc_ast::ast::BlockStatement<'ast>,
    visit: &mut impl FnMut(ScopeId, u32, TopTypeDecl<'ast>),
) {
    let Some(&scope) = binder.block_scopes.get(&(module, block.span.start)) else {
        return;
    };
    // A continued binder can contain the same source offset in an unrelated appended scope.
    if binder.graph.get(scope).and_then(|scope| scope.parent) != Some(parent) {
        return;
    }
    walk_type_decl_statements(binder, module, scope, &block.body, visit);
}

#[allow(clippy::too_many_arguments)]
fn walk_type_decl_for_in_of<'ast>(
    binder: &Binder,
    module: ScopeId,
    parent: ScopeId,
    left: &'ast ForStatementLeft<'ast>,
    right: &'ast Expression<'ast>,
    body: &'ast Statement<'ast>,
    span_start: u32,
    visit: &mut impl FnMut(ScopeId, u32, TopTypeDecl<'ast>),
) {
    walk_type_decl_expression(binder, module, parent, right, visit);
    let Some(&scope) = binder.block_scopes.get(&(module, span_start)) else {
        return;
    };
    if let ForStatementLeft::VariableDeclaration(declaration) = left {
        walk_type_decl_variable(binder, module, scope, declaration, visit);
    }
    walk_type_decl_statement(binder, module, scope, body, visit);
}

fn walk_type_decl_variable<'ast>(
    binder: &Binder,
    module: ScopeId,
    scope: ScopeId,
    declaration: &'ast oxc_ast::ast::VariableDeclaration<'ast>,
    visit: &mut impl FnMut(ScopeId, u32, TopTypeDecl<'ast>),
) {
    for declarator in &declaration.declarations {
        if let Some(initializer) = &declarator.init {
            walk_type_decl_expression(binder, module, scope, initializer, visit);
        }
    }
}

fn walk_type_decl_function<'ast>(
    binder: &Binder,
    module: ScopeId,
    function: &'ast Function<'ast>,
    visit: &mut impl FnMut(ScopeId, u32, TopTypeDecl<'ast>),
) {
    let Some(&scope) = binder.fn_scopes.get(&(module, function.span.start)) else {
        return;
    };
    for parameter in &function.params.items {
        if let Some(initializer) = &parameter.initializer {
            walk_type_decl_expression(binder, module, scope, initializer, visit);
        }
    }
    if let Some(body) = &function.body {
        walk_type_decl_statements(binder, module, scope, &body.statements, visit);
    }
}

fn walk_type_decl_class<'ast>(
    binder: &Binder,
    module: ScopeId,
    scope: ScopeId,
    class: &'ast Class<'ast>,
    visit: &mut impl FnMut(ScopeId, u32, TopTypeDecl<'ast>),
) {
    for element in &class.body.body {
        match element {
            ClassElement::MethodDefinition(method) => {
                walk_type_decl_function(binder, module, &method.value, visit);
            }
            ClassElement::PropertyDefinition(property) => {
                if let Some(initializer) = &property.value {
                    walk_type_decl_expression(binder, module, scope, initializer, visit);
                }
            }
            _ => {}
        }
    }
}

fn walk_type_decl_expression<'ast>(
    binder: &Binder,
    module: ScopeId,
    scope: ScopeId,
    expression: &'ast Expression<'ast>,
    visit: &mut impl FnMut(ScopeId, u32, TopTypeDecl<'ast>),
) {
    match expression {
        Expression::FunctionExpression(function) => {
            walk_type_decl_function(binder, module, function, visit);
        }
        Expression::ArrowFunctionExpression(arrow) => {
            let Some(&scope) = binder.fn_scopes.get(&(module, arrow.span.start)) else {
                return;
            };
            for parameter in &arrow.params.items {
                if let Some(initializer) = &parameter.initializer {
                    walk_type_decl_expression(binder, module, scope, initializer, visit);
                }
            }
            walk_type_decl_statements(binder, module, scope, &arrow.body.statements, visit);
        }
        Expression::ClassExpression(class) => {
            walk_type_decl_class(binder, module, scope, class, visit);
        }
        Expression::NewExpression(new_expression) => {
            walk_type_decl_expression(binder, module, scope, &new_expression.callee, visit);
            for argument in &new_expression.arguments {
                if let Some(argument) = argument.as_expression() {
                    walk_type_decl_expression(binder, module, scope, argument, visit);
                }
            }
        }
        Expression::CallExpression(call) => {
            walk_type_decl_expression(binder, module, scope, &call.callee, visit);
            for argument in &call.arguments {
                if let Some(argument) = argument.as_expression() {
                    walk_type_decl_expression(binder, module, scope, argument, visit);
                }
            }
        }
        Expression::AssignmentExpression(assignment) => {
            walk_type_decl_expression(binder, module, scope, &assignment.right, visit);
        }
        Expression::StaticMemberExpression(member) => {
            walk_type_decl_expression(binder, module, scope, &member.object, visit);
        }
        Expression::ObjectExpression(object) => {
            for property in &object.properties {
                if let ObjectPropertyKind::ObjectProperty(property) = property {
                    walk_type_decl_expression(binder, module, scope, &property.value, visit);
                }
            }
        }
        Expression::ArrayExpression(array) => {
            for element in &array.elements {
                if let Some(element) = element.as_expression() {
                    walk_type_decl_expression(binder, module, scope, element, visit);
                }
            }
        }
        Expression::ParenthesizedExpression(parenthesized) => {
            walk_type_decl_expression(binder, module, scope, &parenthesized.expression, visit);
        }
        Expression::TSAsExpression(assertion) => {
            walk_type_decl_expression(binder, module, scope, &assertion.expression, visit);
        }
        Expression::TSTypeAssertion(assertion) => {
            walk_type_decl_expression(binder, module, scope, &assertion.expression, visit);
        }
        _ => {}
    }
}

/// Allocate fresh type-parameter ids in source order.
/// Names are paired later when lowering with a parameter frame in scope.
pub(in crate::check::checker) fn alloc_type_param_ids(
    decl: Option<&TSTypeParameterDeclaration<'_>>,
    next_type_param: &mut u32,
) -> Vec<TypeParamId> {
    let Some(decl) = decl else {
        return Vec::new();
    };
    decl.params
        .iter()
        .map(|_| {
            let id = TypeParamId(*next_type_param);
            *next_type_param += 1;
            id
        })
        .collect()
}

fn recover_interface_fragment_params(
    slots: &mut BTreeMap<(usize, String), TypeParamId>,
    recovery_params: &mut Vec<TypeParamId>,
    recovery_names: &mut Vec<String>,
    recovery_defaults: &mut Vec<PublishedTypeParameterDefault>,
    fragment_decl: Option<&TSTypeParameterDeclaration<'_>>,
    next_type_param: &mut u32,
) -> Vec<TypeParamId> {
    fragment_decl
        .map(|declaration| declaration.params.as_slice())
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(index, parameter)| {
            let key = (index, parameter.name.name.to_string());
            if let Some(id) = slots.get(&key) {
                *id
            } else {
                let id = TypeParamId(*next_type_param);
                *next_type_param += 1;
                slots.insert(key, id);
                recovery_params.push(id);
                recovery_names.push(parameter.name.name.to_string());
                recovery_defaults.push(PublishedTypeParameterDefault::Absent);
                id
            }
        })
        .collect()
}

fn header_fragment_binding(
    declaration: crate::binder::declaration::DeclId,
    param_decl: Option<&TSTypeParameterDeclaration<'_>>,
    ids: &[TypeParamId],
) -> HeaderFragmentBinding {
    let parameters = param_decl
        .map(|declaration| declaration.params.as_slice())
        .unwrap_or_default();
    assert_eq!(
        parameters.len(),
        ids.len(),
        "one reserved identity per class/interface header parameter"
    );
    HeaderFragmentBinding {
        declaration,
        parameters: parameters
            .iter()
            .zip(ids)
            .map(|(parameter, &id)| NamedTypeParamBinding {
                name: parameter.name.name.to_string(),
                id,
            })
            .collect(),
    }
}

fn sort_header_fragment_bindings(
    binder: &Binder,
    group: TypeGroupId,
    fragments: &mut [HeaderFragmentBinding],
) {
    let bound = binder
        .type_groups
        .get(group)
        .expect("header fragment group is reserved by the binder");
    fragments.sort_by_key(|fragment| {
        bound
            .fragments
            .iter()
            .position(|candidate| candidate.declaration == fragment.declaration)
            .expect("header fragment declaration belongs to its reserved type group")
    });
}

/// The legacy type-storage id a name resolves to from `scope` (binder type slot), if
/// any. Walks the scope graph like value resolution, then reads the `ty` slot.
pub(in crate::check::checker) fn type_decl_id(
    binder: &Binder,
    scope: ScopeId,
    name: &str,
) -> Option<TypeGroupId> {
    let symbol_id = binder.resolve_type(scope, name)?;
    binder.symbols.get(symbol_id).and_then(|s| s.ty)
}

#[cfg(test)]
mod topology_tests {
    use super::*;
    use crate::types::repr::{IntrinsicKind, ObjectType, PropertyType, TypeFlags};
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    fn bind(prelude_source: &str, user_source: &str) -> Binder {
        let prelude_allocator = Allocator::default();
        let user_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, prelude_source, SourceType::ts()).parse();
        let user = Parser::new(&user_allocator, user_source, SourceType::ts()).parse();
        assert!(!prelude.panicked && !user.panicked);
        crate::binder::bind_module_with_prelude(&prelude.program, &user.program)
    }

    #[test]
    fn source_array_group_wins_before_builtin_opaque_fallback() {
        let source = bind(
            "interface Array<T> { preludeElement: T }",
            "interface Array<T> { sourceElement: T }",
        );
        let source_group =
            type_decl_id(&source, source.module, "Array").expect("source Array group");
        assert!(source_group.0 >= source.prelude_type_group_count);
        assert_eq!(
            topology_segments_group(&source, source.module, &["Array"]),
            Ok(source_group)
        );

        let builtin = bind("interface Array<T> { preludeElement: T }", "");
        let builtin_group =
            type_decl_id(&builtin, builtin.module, "Array").expect("prelude Array group");
        assert!(builtin_group.0 < builtin.prelude_type_group_count);
        assert_eq!(
            topology_segments_group(&builtin, builtin.module, &["Array"]),
            Err(HeritageTypePlan::Opaque(BTreeSet::new()))
        );
    }

    #[test]
    fn local_interface_heritage_keeps_frozen_html_element_endpoint_complete() {
        let binder = bind(
            "interface HTMLElement { elementMarker: string }",
            "interface Local extends HTMLElement { localMarker: number }",
        );
        let html_element = type_decl_id(&binder, binder.module, "HTMLElement")
            .expect("frozen HTMLElement group remains visible");
        assert!(html_element.0 < binder.prelude_type_group_count);
        let published = (0..binder.prelude_type_group_count)
            .map(|_| PublishedTypeDecl {
                params: Vec::new(),
                defaults: Vec::new(),
            })
            .collect::<Vec<_>>()
            .into();
        let declarations = TypeDeclTable::with_published(published);

        assert_eq!(
            plan_heritage_group_application(
                &binder,
                &declarations,
                binder.module,
                html_element,
                None,
                &BTreeMap::new(),
                &mut BTreeSet::new(),
            ),
            HeritageTypePlan::complete(BTreeSet::new()),
        );
    }

    #[test]
    fn object_alias_graph_scales_with_local_suffix_and_preserves_reachability() {
        const FROZEN_ROWS: usize = 4096;

        let mut base = Store::new();
        for _ in 0..FROZEN_ROWS {
            base.push_intrinsic(IntrinsicKind::Any, TypeFlags::EMPTY);
        }
        base.freeze_as_base().expect("scaled type base seals");
        let mut delta = base.fork_delta().expect("local type suffix");

        let active = delta.push_object(ObjectType::default(), TypeFlags::EMPTY);
        let holder = delta.push_object(
            ObjectType {
                properties: vec![PropertyType::public("active", active)],
                ..Default::default()
            },
            TypeFlags::EMPTY,
        );
        let root = delta.push_object(
            ObjectType {
                properties: vec![PropertyType::public("holder", holder)],
                ..Default::default()
            },
            TypeFlags::EMPTY,
        );
        let active_set = FxHashSet::from_iter([active]);
        let graph = ObjectAliasCanonicalizationGraph::build(&delta, &active_set);

        assert_eq!(graph.local_start, FROZEN_ROWS);
        assert_eq!(graph.reverse.len(), 3);
        assert_eq!(graph.reaches_active_reservation.len(), 3);
        assert_eq!(graph.scanned_owners, 3);
        assert!(graph.has_external_store_inbound(active));
        assert!(graph.body_reaches_active_reservation(&delta, root));
        assert_eq!(
            graph.active_reservations_reachable_from_semantic_roots(
                &delta,
                [TypeId(0), root],
                &active_set,
            ),
            active_set,
        );
        assert!(graph
            .active_reservations_reachable_from_semantic_roots(
                &delta,
                [TypeId(0)],
                &FxHashSet::from_iter([active]),
            )
            .is_empty());
    }

    #[test]
    fn class_allocation_manifest_records_identity_before_publication() {
        reset_class_allocation_events_for_test();
        let mut next_class_id = 41;

        let class = reserve_class_id(&mut next_class_id);

        assert_eq!(class, ClassId(41));
        assert_eq!(next_class_id, 42);
        assert_eq!(class_allocation_events_for_test(), vec![ClassId(41)]);
    }
}

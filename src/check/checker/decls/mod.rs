//! decls module (extracted from checker/mod.rs).

use super::context::*;
use super::lexical_events::InterfaceOccurrenceKind;
use super::type_groups::{
    InterfaceAlternativeKind, InterfaceTypedAlternative, PublishedTypeParameterDefault,
};
use crate::binder::declaration::TypeGroupId;
use crate::binder::scope::ScopeId;
use crate::binder::Binder;
use crate::diagnostics::Diagnostic;
use crate::span::Span;
use crate::types::repr::{ClassId, TypeParamId, TypeTag};
use crate::types::store::TypeId;
use crate::types::Interner;
use oxc_ast::ast::{
    Class, ClassElement, Declaration, Expression, ForStatementInit, ForStatementLeft, Function,
    ObjectPropertyKind, Program, Statement, TSInterfaceDeclaration, TSInterfaceHeritage,
    TSModuleDeclaration, TSModuleDeclarationBody, TSType, TSTypeAliasDeclaration, TSTypeName,
    TSTypeParameterDeclaration, TSTypeParameterInstantiation,
};
use oxc_span::GetSpan;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::{BTreeMap, BTreeSet};

mod interface;
mod params;
mod resolve;

#[derive(Default)]
struct InterfaceOwnMemberOwners {
    properties: BTreeMap<String, (super::events::RecordTicket, Span)>,
    string_index: Option<(super::events::RecordTicket, Span)>,
    number_index: Option<(super::events::RecordTicket, Span)>,
}

#[derive(Copy, Clone)]
struct InterfaceHeritageDiagnostic<'name> {
    owner: super::events::RecordTicket,
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

impl<'a, 'ast> Pass<'a, 'ast> {
    fn lower_type_group_parameter_metadata(&mut self, index: usize) {
        let (scope, param_decl, params, interface_declaration) = match self.type_decls.get(index) {
            Some(TypeDecl::Interface {
                declaration,
                scope,
                param_decl,
                params,
                ..
            }) => (*scope, *param_decl, params.clone(), Some(*declaration)),
            Some(TypeDecl::Alias {
                scope,
                param_decl,
                params,
                ..
            }) => (*scope, *param_decl, params.clone(), None),
            _ => return,
        };
        let group = TypeGroupId(u32::try_from(index).expect("type group index fits u32"));
        let frame = self.build_type_param_frame(param_decl, &params);
        let validate_locally = interface_declaration.is_none();
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
        let descriptors = if let Some(declaration) = interface_declaration {
            let header_span = self
                .binder
                .declarations
                .get(declaration)
                .map(|declaration| declaration.site.binding_span)
                .unwrap_or(Span::new(0, 0));
            let owner = self
                .lexical_events
                .interface_occurrence_owner(
                    declaration,
                    InterfaceOccurrenceKind::Header,
                    header_span.start,
                )
                .expect("interface header has one exact preallocated owner");
            self.with_ticket_effects(owner, lower)
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
        let declaration = match self.type_decls.get(decl_id.index()) {
            Some(TypeDecl::Interface { declaration, .. })
            | Some(TypeDecl::Alias { declaration, .. })
            | Some(TypeDecl::Class { declaration, .. })
            | Some(TypeDecl::UnsupportedClassInterface { declaration, .. })
            | Some(TypeDecl::Unavailable { declaration }) => *declaration,
            Some(TypeDecl::Resolved { .. }) | None => {
                panic!("published prelude groups must not re-enter private construction")
            }
        };
        let owner = self
            .lexical_events
            .declaration_owner(declaration)
            .expect("type declaration must have a preallocated lexical owner");
        self.with_ticket_effects(owner.ticket, produce)
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
        self.fill_type_decls_range(scope, 0, self.type_decls.len());
    }

    pub(in crate::check::checker) fn fill_type_decls_range(
        &mut self,
        scope: ScopeId,
        start: usize,
        end: usize,
    ) {
        // Template lowering keeps conditionals lazy until value-position demand.
        self.building_template = true;

        for index in start..end {
            self.lower_type_group_parameter_metadata(index);
        }

        // Freeze interface dependency components before aliases can observe them.
        self.construct_pending_interface_sccs(start, end);

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

        // Fill seeded object aliases so legal member recursion resolves to the reserved id.
        for index in start..end {
            self.ensure_object_alias_filled(scope, index);
        }

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
        self.building_template = false;
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

    /// Construct ready interface components privately. Every member and heritage
    /// annotation in an SCC is lowered before any reserved root is filled.
    fn construct_pending_interface_sccs(&mut self, start: usize, end: usize) {
        let end = end.min(self.type_decls.len());
        let topology = interface_heritage_topology(self.binder, &self.type_decls);
        let components = interface_sccs(&self.type_decls, start, end, &topology);
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

        loop {
            let mut progressed = false;
            let mut deferred = Vec::new();
            for component in remaining {
                if self.interface_component_is_ready(&component, &topology) {
                    let cyclic_heritage =
                        interface_component_has_cycle(&self.type_decls, &component, &topology);
                    self.construct_interface_component(&component, cyclic_heritage, &topology);
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
            let mut first_method_members = BTreeSet::new();
            let mut lowered_fragments = Vec::with_capacity(fragments.len());
            for fragment in fragments {
                let frame = self.build_type_param_frame(fragment.param_decl, &fragment.params);
                let fragment_own = self.with_type_params(frame, |pass| {
                    pass.lower_interface_declaration_members(
                        fragment.declaration,
                        fragment.scope,
                        fragment.members,
                    )
                });
                let mut seen_names = BTreeSet::new();
                let fragment_methods = fragment
                    .members
                    .iter()
                    .filter_map(|member| match member {
                        oxc_ast::ast::TSSignature::TSPropertySignature(signature)
                            if !signature.computed =>
                        {
                            signature
                                .key
                                .static_name()
                                .map(|name| (name.into_owned(), false))
                        }
                        oxc_ast::ast::TSSignature::TSMethodSignature(signature)
                            if !signature.computed =>
                        {
                            signature
                                .key
                                .static_name()
                                .map(|name| (name.into_owned(), true))
                        }
                        _ => None,
                    })
                    .filter_map(|(name, method)| {
                        seen_names.insert(name.clone()).then_some((name, method))
                    })
                    .filter_map(|(name, method)| method.then_some(name))
                    .collect::<BTreeSet<_>>();
                lowered_fragments.push((fragment, fragment_own.clone()));
                own = self.merge_interface_fragment_members(
                    own,
                    fragment_own,
                    &mut first_method_members,
                    &fragment_methods,
                );
            }
            let alternatives = self.validate_interface_fragment_conflicts(&lowered_fragments);
            own_objects.push((index, own, alternatives));
        }

        let component_set: FxHashSet<usize> = component.iter().copied().collect();
        let mut completed = Vec::with_capacity(component.len());
        for (index, own, mut alternatives) in own_objects {
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
            let mut bases = crate::types::repr::ObjectType::default();
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
                        self.with_ticket_effects(owner, |pass| {
                            pass.with_type_params(frame, |pass| match plan {
                                InterfaceHeritagePlan::Complete(_) => pass
                                    .validate_interface_heritage_application_without_resolution(
                                        fragment.scope,
                                        heritage,
                                    ),
                                InterfaceHeritagePlan::Opaque(_) => {
                                    pass.record_opaque_interface_heritage(fragment.scope, heritage)
                                }
                                InterfaceHeritagePlan::Poisoned => {}
                            })
                        });
                        // Cyclic bases are invalid. Their annotations still own all
                        // diagnostics, but their members never cross the SCC boundary.
                        continue;
                    }
                    let base = self.with_ticket_effects(owner, |pass| {
                        pass.with_type_params(frame, |pass| match plan {
                            InterfaceHeritagePlan::Complete(_) => {
                                pass.ensure_heritage_base_filled(fragment.scope, heritage);
                                pass.resolve_interface_heritage_object(fragment.scope, heritage)
                            }
                            InterfaceHeritagePlan::Poisoned => {
                                pass.diagnose_poisoned_interface_heritage(fragment.scope, heritage);
                                None
                            }
                            InterfaceHeritagePlan::Opaque(_) => {
                                pass.record_opaque_interface_heritage(fragment.scope, heritage);
                                None
                            }
                        })
                    });
                    if let Some(base) = base {
                        heritage_surfaces.push((
                            owner,
                            heritage_span,
                            heritage_display_name(heritage),
                            base.clone(),
                        ));
                        bases = interface::merge_object_members_first(bases, base);
                    }
                }
            }
            alternatives.extend(self.validate_interface_heritage_conflicts(
                &heritage_surfaces,
                canonical_owner,
                canonical_span,
            ));
            let own_surface = own.clone();
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
            self.template_fill[index] = ClassFillState::Done;
            self.freeze_type_group(TypeGroupId(
                u32::try_from(index).expect("type group index fits u32"),
            ));
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
            } => parameter_descriptors
                .clone()
                .expect("canonical interface descriptors lower before group validation"),
            _ => unreachable!("interface header validation owns an interface draft"),
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
            let owner = self
                .lexical_events
                .interface_occurrence_owner(
                    fragment.declaration,
                    InterfaceOccurrenceKind::Header,
                    header_span.start,
                )
                .expect("interface header has one exact preallocated owner");
            let shape = self.with_ticket_effects(owner, |pass| {
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
        let TypeDecl::Interface {
            recovery_params,
            recovery_defaults,
            ..
        } = &mut self.type_decls[index]
        else {
            unreachable!("interface header validation owns an interface draft")
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
                let owner = self
                    .lexical_events
                    .interface_occurrence_owner(
                        declaration,
                        InterfaceOccurrenceKind::Header,
                        span.start,
                    )
                    .expect("merged interface header has one exact preallocated owner");
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
            let owner = self
                .lexical_events
                .interface_occurrence_owner(
                    fragment.declaration,
                    InterfaceOccurrenceKind::Header,
                    header_span.start,
                )
                .expect("interface header has one exact preallocated owner");
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
                let owner = self
                    .lexical_events
                    .interface_occurrence_owner(
                        declaration,
                        InterfaceOccurrenceKind::Header,
                        span.start,
                    )
                    .expect("merged interface header has one exact preallocated owner");
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
            let owner = self
                .lexical_events
                .interface_occurrence_owner(
                    declaration,
                    InterfaceOccurrenceKind::Header,
                    span.start,
                )
                .expect("merged interface header has one exact preallocated owner");
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
        struct Member {
            owner: super::events::RecordTicket,
            span: Span,
            kind: MemberKind,
            ty: TypeId,
            optional: bool,
            readonly: bool,
            name: String,
        }
        #[derive(Clone)]
        struct Index {
            owner: super::events::RecordTicket,
            span: Span,
            ty: TypeId,
        }

        let mut members: BTreeMap<String, Vec<Member>> = BTreeMap::new();
        let mut all_properties = Vec::new();
        let mut string_index: Option<Index> = None;
        let mut number_index: Option<Index> = None;
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
                    oxc_ast::ast::TSSignature::TSPropertySignature(signature)
                        if !signature.computed =>
                    {
                        let Some(name) = signature.key.static_name().map(|name| name.into_owned())
                        else {
                            continue;
                        };
                        let Some(property) = object.property(&name) else {
                            continue;
                        };
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
                        all_properties.push(member.clone());
                        if let Some(first) = members.get(&name).and_then(|items| items.first()) {
                            if first.kind != member.kind {
                                if first.ty != member.ty {
                                    let source = self.interner.intern_object(
                                        crate::types::repr::ObjectType {
                                            properties: vec![
                                                crate::types::repr::PropertyType::public(
                                                    name.clone(),
                                                    first.ty,
                                                ),
                                            ],
                                            ..Default::default()
                                        },
                                    );
                                    let target = self.interner.intern_object(
                                        crate::types::repr::ObjectType {
                                            properties: vec![
                                                crate::types::repr::PropertyType::public(
                                                    name.clone(),
                                                    member.ty,
                                                ),
                                            ],
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
                                        crate::types::repr::PropertyType::public(
                                            name.clone(),
                                            first.ty,
                                        );
                                    first_property.optional = first.optional;
                                    first_property.readonly = first.readonly;
                                    let mut later_property =
                                        crate::types::repr::PropertyType::public(
                                            name.clone(),
                                            member.ty,
                                        );
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
                        members.entry(name).or_default().push(member);
                    }
                    oxc_ast::ast::TSSignature::TSMethodSignature(signature)
                        if !signature.computed =>
                    {
                        let Some(name) = signature.key.static_name().map(|name| name.into_owned())
                        else {
                            continue;
                        };
                        let Some(property) = object.property(&name) else {
                            continue;
                        };
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
                        if let Some(first) = members.get(&name).and_then(|items| items.first()) {
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
                        members.entry(name).or_default().push(member);
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
            .filter_map(|(name, occurrences)| {
                (occurrences.len() > 1).then(|| InterfaceTypedAlternative {
                    kind: InterfaceAlternativeKind::Member,
                    key: name,
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
        surfaces: &[(
            super::events::RecordTicket,
            Span,
            String,
            crate::types::repr::ObjectType,
        )],
        diagnostic_owner: super::events::RecordTicket,
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
                    let Some(right_property) = right.property(&left_property.name) else {
                        continue;
                    };
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
                        key: left_property.name.clone(),
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
    ) -> InterfaceOwnMemberOwners {
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
                    oxc_ast::ast::TSSignature::TSPropertySignature(signature)
                        if !signature.computed =>
                    {
                        if let Some(name) = signature.key.static_name() {
                            owners
                                .properties
                                .entry(name.into_owned())
                                .or_insert((owner, span));
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
        surfaces: &[(
            super::events::RecordTicket,
            Span,
            String,
            crate::types::repr::ObjectType,
        )],
        diagnostic: InterfaceHeritageDiagnostic<'_>,
        own: &crate::types::repr::ObjectType,
        own_owners: &InterfaceOwnMemberOwners,
    ) -> Vec<InterfaceTypedAlternative> {
        let mut alternatives = Vec::new();
        for (_, _, base_name, base) in surfaces {
            for own_property in &own.properties {
                let Some(base_property) = base.property(&own_property.name) else {
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
                    key: own_property.name.clone(),
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
                let own_property = own_owners.properties.get(&property.name).copied();
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
                            name: property.name.clone(),
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
    fn ensure_object_alias_filled(&mut self, scope: ScopeId, index: usize) {
        if !matches!(self.template_fill.get(index), Some(ClassFillState::Pending)) {
            return;
        }
        let decl_id = TypeGroupId(u32::try_from(index).expect("type group index fits u32"));
        self.with_type_decl_effects(decl_id, |pass| {
            pass.ensure_object_alias_filled_inner(scope, index)
        });
    }

    fn ensure_object_alias_filled_inner(&mut self, _scope: ScopeId, index: usize) {
        match self.template_fill.get(index).copied() {
            Some(ClassFillState::Done) | Some(ClassFillState::Filling) => return,
            Some(ClassFillState::Pending) => {}
            None => return,
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
            _ => return,
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
        self.freeze_type_group(decl_id);
    }

    /// Force-fill a heritage base before composition reads its members.
    /// Interfaces recurse, while aliases resolve then fill any reserved template
    /// they land on. Generic alias heritage resolves on demand at instantiation time.
    fn ensure_heritage_base_filled(&mut self, scope: ScopeId, heritage: &TSInterfaceHeritage<'_>) {
        let Some((decl_id, _, _)) =
            interface::interface_heritage_root(self.binder, scope, heritage)
        else {
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
        let target = self.type_decls.iter().position(|decl| match decl {
            TypeDecl::Interface { reserved, .. } => *reserved == ty,
            TypeDecl::Alias {
                object_template: Some(reserved),
                ..
            } => *reserved == ty,
            _ => false,
        });
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
    declarations: &[TypeDecl<'_>],
) -> InterfaceHeritageTopology {
    let mut topology = InterfaceHeritageTopology::default();
    for declaration in declarations {
        let TypeDecl::Interface { fragments, .. } = declaration else {
            continue;
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
    declarations: &[TypeDecl<'_>],
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
    declarations: &[TypeDecl<'_>],
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
    declarations: &[TypeDecl<'_>],
    scope: ScopeId,
    group: TypeGroupId,
    arguments: Option<&TSTypeParameterInstantiation<'_>>,
    symbols: &BTreeMap<String, HeritageTypePlan>,
    aliases: &mut BTreeSet<TypeGroupId>,
) -> HeritageTypePlan {
    let Some(declaration) = declarations.get(group.index()) else {
        return HeritageTypePlan::Opaque(BTreeSet::new());
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
        }
        | TypeDecl::Class {
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
        TypeDecl::UnsupportedClassInterface { params, .. } | TypeDecl::Resolved { params } => {
            (params.len(), params.len())
        }
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
        TypeDecl::UnsupportedClassInterface { .. }
        | TypeDecl::Unavailable { .. }
        | TypeDecl::Resolved { .. } => HeritageTypePlan::complete(BTreeSet::new()),
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
        ["Array"] => Err(HeritageTypePlan::Opaque(BTreeSet::new())),
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
    declarations: &[TypeDecl<'_>],
    start: usize,
    end: usize,
    topology: &InterfaceHeritageTopology,
) -> Vec<Vec<usize>> {
    let nodes: BTreeSet<TypeGroupId> = (start..end)
        .filter(|index| matches!(declarations.get(*index), Some(TypeDecl::Interface { .. })))
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

fn interface_component_has_cycle(
    declarations: &[TypeDecl<'_>],
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
    decls: &mut Vec<TypeDecl<'ast>>,
    resolved: &mut [Option<TypeId>],
) {
    // The AST walk is joined to the binder through the exact lexical declaration,
    // never by selecting a first/last declaration from a same-name group.
    walk_type_decls(
        binder,
        module,
        program,
        &mut |_walk_scope, _, declaration| {
            match declaration {
                TopTypeDecl::Interface(iface) => {
                    let Some((group, declaration, scope)) = exact_type_fragment_at(
                        binder,
                        module,
                        crate::binder::declaration::TypeFragmentKind::Interface,
                        iface.id.span.start,
                    ) else {
                        return;
                    };
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
                            declaration: class_declaration,
                            class_id,
                            params,
                            param_decl,
                            ..
                        }) => {
                            let mut header_fragments = vec![header_fragment_binding(
                                *class_declaration,
                                *param_decl,
                                params,
                            )];
                            let interface_header = recover_header_fragment_binding(
                                declaration,
                                fragment.param_decl,
                                &header_fragments,
                                next_type_param,
                            );
                            fragment.params = header_parameter_ids(&interface_header);
                            header_fragments.push(interface_header);
                            sort_header_fragment_bindings(binder, group, &mut header_fragments);
                            let replacement = TypeDecl::UnsupportedClassInterface {
                                declaration: *class_declaration,
                                class_id: *class_id,
                                params: params.clone(),
                                header_fragments,
                            };
                            *decls.get_mut(group.index()).expect("type group slot") = replacement;
                            if let Some(slot) = resolved.get_mut(group.index()) {
                                *slot = None;
                            }
                        }
                        Some(TypeDecl::UnsupportedClassInterface {
                            header_fragments, ..
                        }) => {
                            let interface_header = recover_header_fragment_binding(
                                declaration,
                                fragment.param_decl,
                                header_fragments,
                                next_type_param,
                            );
                            fragment.params = header_parameter_ids(&interface_header);
                            header_fragments.push(interface_header);
                            sort_header_fragment_bindings(binder, group, header_fragments);
                        }
                        Some(TypeDecl::Resolved { .. }) => {
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
                                recovery_defaults: vec![
                                    PublishedTypeParameterDefault::Absent;
                                    fragment.params.len()
                                ],
                                param_slots,
                                conflict_alternatives: Vec::new(),
                                defaults,
                                parameter_descriptors: None,
                                param_decl: iface.type_parameters.as_deref(),
                                extends: &iface.extends,
                                fragments: vec![fragment],
                            };
                        }
                        Some(TypeDecl::Alias { .. }) | Some(TypeDecl::Unavailable { .. }) => {
                            decls[group.index()] = TypeDecl::Unavailable { declaration };
                            if let Some(slot) = resolved.get_mut(group.index()) {
                                *slot = None;
                            }
                        }
                        None => unreachable!("type group slot was extended"),
                    }
                }
                TopTypeDecl::Alias(alias) => {
                    let exact = exact_type_fragment_at(
                        binder,
                        module,
                        crate::binder::declaration::TypeFragmentKind::TypeAlias,
                        alias.id.span.start,
                    );
                    let (group, declaration, scope) = match exact {
                        Some(exact) => (Some(exact.0), Some(exact.1), exact.2),
                        None => (None, None, _walk_scope),
                    };
                    let params =
                        alloc_type_param_ids(alias.type_parameters.as_deref(), next_type_param);
                    let defaults = vec![None; params.len()];
                    // M25: a top-level conditional-type body reserves a conditional template
                    // id and seeds `type_resolved`, so a self-recursive reference resolves to
                    // it (as a lazy instantiation) rather than expanding at lowering. The
                    // placeholder is filled in the fill step.
                    let conditional_template = if matches!(
                        alias.type_annotation,
                        oxc_ast::ast::TSType::TSConditionalType(_)
                    ) {
                        let reserved = interner.reserve_conditional();
                        // M28 round 3: name the reserved row so a deferred
                        // instantiation renders by alias NAME, not the raw body.
                        interner.set_template_name(reserved, alias.id.name.as_str());
                        if let Some(group) = group {
                            if let Some(slot) = resolved.get_mut(group.index()) {
                                *slot = Some(reserved);
                            }
                        }
                        Some(reserved)
                    } else {
                        None
                    };
                    // Top-level mapped aliases reserve a template id so self-recursive
                    // references resolve as lazy instantiations, not error types.
                    let mapped_template =
                        if matches!(alias.type_annotation, oxc_ast::ast::TSType::TSMappedType(_)) {
                            let reserved = interner.reserve_mapped();
                            // M28 round 3: named for rendering, like the conditional row.
                            interner.set_template_name(reserved, alias.id.name.as_str());
                            if let Some(group) = group {
                                if let Some(slot) = resolved.get_mut(group.index()) {
                                    *slot = Some(reserved);
                                }
                            }
                            Some(reserved)
                        } else {
                            None
                        };
                    // Non-generic object-literal aliases reserve an object id so member
                    // self-references are legal recursion. Generic object aliases remain
                    // structural templates instantiated by substitution, so they are not seeded.
                    let object_template = if alias.type_parameters.is_none()
                        && matches!(
                            alias.type_annotation,
                            oxc_ast::ast::TSType::TSTypeLiteral(_)
                        ) {
                        let reserved = interner.reserve_object();
                        if let Some(group) = group {
                            if let Some(slot) = resolved.get_mut(group.index()) {
                                *slot = Some(reserved);
                            }
                        }
                        Some(reserved)
                    } else {
                        None
                    };
                    if let (Some(group), Some(declaration)) = (group, declaration) {
                        ensure_type_group_slot(decls, group.index());
                        if !matches!(decls.get(group.index()), Some(TypeDecl::Resolved { .. })) {
                            decls[group.index()] = TypeDecl::Unavailable { declaration };
                            if let Some(slot) = resolved.get_mut(group.index()) {
                                *slot = None;
                            }
                        } else {
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
                TopTypeDecl::Class(class) if class.id.is_some() => {
                    // M13: a fresh stable `ClassId` for this declaration (source order),
                    // stamped onto its members during class publication.
                    let class_id = ClassId(*next_class_id);
                    *next_class_id += 1;
                    if let Some(id) = &class.id {
                        let exact = exact_type_fragment_at(
                            binder,
                            module,
                            crate::binder::declaration::TypeFragmentKind::Class,
                            id.span.start,
                        );
                        let (group, declaration, scope) = match exact {
                            Some(exact) => (Some(exact.0), Some(exact.1), exact.2),
                            None => (None, None, _walk_scope),
                        };
                        if let (Some(group), Some(declaration)) = (group, declaration) {
                            ensure_type_group_slot(decls, group.index());
                            match decls.get(group.index()) {
                                Some(TypeDecl::Interface { fragments, .. }) => {
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
                                    let class_header = recover_header_fragment_binding(
                                        declaration,
                                        class.type_parameters.as_deref(),
                                        &header_fragments,
                                        next_type_param,
                                    );
                                    let params = header_parameter_ids(&class_header);
                                    header_fragments.push(class_header);
                                    sort_header_fragment_bindings(
                                        binder,
                                        group,
                                        &mut header_fragments,
                                    );
                                    decls[group.index()] = TypeDecl::UnsupportedClassInterface {
                                        declaration,
                                        class_id,
                                        params,
                                        header_fragments,
                                    };
                                    if let Some(slot) = resolved.get_mut(group.index()) {
                                        *slot = None;
                                    }
                                }
                                Some(TypeDecl::Resolved { .. }) => {
                                    // M16: allocate one id per declared type parameter (in source
                                    // order), paired with names when the class body is lowered.
                                    let params = alloc_type_param_ids(
                                        class.type_parameters.as_deref(),
                                        next_type_param,
                                    );
                                    decls[group.index()] = TypeDecl::Class {
                                        declaration,
                                        scope,
                                        class_id,
                                        params,
                                        param_decl: class.type_parameters.as_deref(),
                                        class,
                                    };
                                }
                                Some(_) => {
                                    // Preserve reservation monotonicity for rejected duplicate
                                    // class/type compositions even though their surface is absent.
                                    let _ = alloc_type_param_ids(
                                        class.type_parameters.as_deref(),
                                        next_type_param,
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
                _ => {}
            }
        },
    );
}

fn ensure_type_group_slot<'ast>(decls: &mut Vec<TypeDecl<'ast>>, index: usize) {
    while decls.len() < index {
        decls.push(TypeDecl::Resolved { params: Vec::new() });
    }
    if decls.len() == index {
        decls.push(TypeDecl::Resolved { params: Vec::new() });
    }
}

pub(in crate::check::checker) fn exact_type_fragment_at(
    binder: &Binder,
    module: ScopeId,
    kind: crate::binder::declaration::TypeFragmentKind,
    binding_start: u32,
) -> Option<(TypeGroupId, crate::binder::declaration::DeclId, ScopeId)> {
    let source = binder
        .namespaces
        .source_units()
        .find(|unit| unit.module == module)
        .map(|unit| unit.source);
    binder.type_groups.iter().find_map(|group| {
        group.fragments.iter().find_map(|fragment| {
            (source.map_or(fragment.site.module == module, |source| {
                fragment.source == source
            }) && fragment.kind == kind
                && fragment.site.binding_span.start == binding_start)
                .then_some((group.id, fragment.declaration, fragment.scope))
        })
    })
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
    walk_type_decl_statements(binder, module, module, &program.body, visit);
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
                crate::binder::declaration::TypeFragmentKind::Interface,
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
                crate::binder::declaration::TypeFragmentKind::TypeAlias,
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
                    crate::binder::declaration::TypeFragmentKind::Class,
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
            walk_type_decl_statements(binder, module, scope, &global.body.body, visit)
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
                    crate::binder::declaration::TypeFragmentKind::Interface,
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
                        crate::binder::declaration::TypeFragmentKind::TypeAlias,
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
                            crate::binder::declaration::TypeFragmentKind::Class,
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
                    walk_type_decl_statements(binder, module, scope, &global.body.body, visit)
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

#[allow(clippy::too_many_arguments)]
fn visit_bound_type<'ast>(
    binder: &Binder,
    module: ScopeId,
    fallback_scope: ScopeId,
    owner_start: u32,
    kind: crate::binder::declaration::TypeFragmentKind,
    binding_start: u32,
    declaration: TopTypeDecl<'ast>,
    visit: &mut impl FnMut(ScopeId, u32, TopTypeDecl<'ast>),
) {
    let source = binder
        .namespaces
        .source_units()
        .find(|unit| unit.module == module)
        .map(|unit| unit.source);
    let matched = binder
        .type_groups
        .iter()
        .flat_map(|group| group.fragments.iter())
        .find(|fragment| {
            source.map_or(fragment.site.module == module, |source| {
                fragment.source == source
            }) && fragment.kind == kind
                && fragment.site.binding_span.start == binding_start
        })
        .map(|fragment| fragment.scope);
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
    debug_assert_eq!(
        binder.graph.get(scope).and_then(|scope| scope.parent),
        Some(parent)
    );
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

fn recover_header_fragment_binding(
    declaration: crate::binder::declaration::DeclId,
    param_decl: Option<&TSTypeParameterDeclaration<'_>>,
    existing: &[HeaderFragmentBinding],
    next_type_param: &mut u32,
) -> HeaderFragmentBinding {
    let mut slots = BTreeMap::new();
    for fragment in existing {
        for (index, parameter) in fragment.parameters.iter().enumerate() {
            let key = (index, parameter.name.clone());
            if let Some(previous) = slots.insert(key, parameter.id) {
                assert_eq!(
                    previous, parameter.id,
                    "matching class/interface recovery slots share one identity"
                );
            }
        }
    }
    let parameters = param_decl
        .map(|declaration| declaration.params.as_slice())
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(index, parameter)| {
            let name = parameter.name.name.to_string();
            let id = slots
                .get(&(index, name.clone()))
                .copied()
                .unwrap_or_else(|| {
                    let id = TypeParamId(*next_type_param);
                    *next_type_param += 1;
                    id
                });
            NamedTypeParamBinding { name, id }
        })
        .collect();
    HeaderFragmentBinding {
        declaration,
        parameters,
    }
}

fn header_parameter_ids(binding: &HeaderFragmentBinding) -> Vec<TypeParamId> {
    binding
        .parameters
        .iter()
        .map(|parameter| parameter.id)
        .collect()
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

/// The value-storage id a name resolves to from `scope` (binder value slot),
/// if any (M11 — the class constructor side). Mirrors [`type_decl_id`] for the value
/// space.
pub(in crate::check::checker) fn value_decl_id(
    binder: &Binder,
    scope: ScopeId,
    name: &str,
) -> Option<crate::binder::declaration::ValueStorageId> {
    let symbol_id = binder.resolve_value(scope, name)?;
    binder.symbols.get(symbol_id).and_then(|s| s.value)
}

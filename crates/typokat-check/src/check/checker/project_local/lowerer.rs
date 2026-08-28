//! Direct immutable-product semantic lowering.

use super::project_local_pass_port::{
    keyof_of_type, substitute, Accessibility, AliasId, ConditionalType, DeclarationKey,
    DeclarationPayload, FunctionType, FxHashMap, GenericTypeParam, Interner, IntrinsicMarker,
    KeywordKind, LibraryDeclarationProduct, LiteralRecipe, MappedModifier, MappedType, MemberKey,
    MemberKind, NormalizedCheckerRecordForTest, NormalizedIncompleteForTest, NormalizedSpanForTest,
    ObjectType, ParameterPattern, ParameterType, ProjectLocalTypeOverlayDraft, PropertyKey,
    PropertyNameRecipe, PropertyType, PublishedAliasSurface, PublishedAliasTerminal,
    PublishedAliasUnavailable, PublishedAliases, QualifiedPathResolutionOutcome, QualifiedPathRole,
    QualifiedPathUnresolvedCause, SignatureKey, SourceSite, TemplateType, TupleRestType, TupleType,
    TypeGroupId, TypeId, TypeOperator, TypeParamId, TypeParameterKey, TypeRecipeKey,
    TypeRecipeNode, TypeReferenceResolution, TypeTag, ValueReferenceResolution, Visibility,
};
#[cfg(any(test, feature = "test-utils"))]
use super::project_local_pass_port::{
    render_type, DemandOutcome, PublishedClasses, SemanticQueryCoordinator, SemanticQueryState,
};
use super::recipe_primitives::{
    build_contextual_type_parameter_index, group_has_runtime_value, has_unsupported_computed_name,
    infer_count, is_type_parameter_reference, mapped_modifier, member_visibility,
    merge_object_surface, property_key,
};
use super::record_staging::{
    LibraryRecordAccumulator, LibraryRecordEventIdentity, LibraryRecordOwnerIdentity,
    LibraryRecordProducer, ProjectLocalLibraryRecordEvent,
};
use super::validation::{
    ProjectLocalConstraintValidation, ProjectLocalHeritageBase, ProjectLocalHeritageValidation,
    ProjectLocalQualifiedPathDiagnosticKind, ProjectLocalQualifiedPathValidation,
    ProjectLocalValidationAccumulator, ProjectLocalValidationCandidate,
    ProjectLocalValidationEvent, ProjectLocalValidationProducer, ProjectLocalValidationSite,
};
use super::ProjectLocalLibraryRecordForTest;
#[cfg(any(test, feature = "test-utils"))]
use super::{
    ProjectLocalModuleTypeProbeForTest, ProjectLocalSemanticProbesForTest,
    ProjectLocalValueProbeForTest,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProductLoweringError {
    UnsupportedRecipe,
    DeferredMember,
    RecursiveAliasWithoutReservation,
    MissingAdmission(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GroupState {
    Pending,
    Resolving,
    Ready,
    Unavailable,
}

pub(super) struct ProductLoweringReceipt {
    pub(super) group_types: Vec<Option<TypeId>>,
    pub(super) group_value_types: Vec<Option<TypeId>>,
    pub(super) ready: BTreeSet<usize>,
    pub(super) value_ready: BTreeSet<usize>,
    pub(super) unavailable: BTreeSet<usize>,
    pub(super) value_unavailable: BTreeSet<usize>,
    pub(super) type_parameter_ids: Vec<TypeParamId>,
    pub(super) type_parameter_defaults:
        Vec<super::project_local_pass_port::PublishedTypeParameterDefault>,
    pub(super) aliases: PublishedAliases,
    pub(super) overlays: Vec<(TypeGroupId, ProjectLocalTypeOverlayDraft)>,
    pub(super) missing_admissions: BTreeSet<usize>,
    pub(super) library_record_events: Vec<ProjectLocalLibraryRecordEvent>,
    pub(super) validation_events: Vec<ProjectLocalValidationEvent>,
}

pub(super) struct ProductLoweringSeed<'a> {
    pub(super) group_types: &'a [Option<TypeId>],
    pub(super) group_value_types: &'a [Option<TypeId>],
    pub(super) ready: &'a BTreeSet<usize>,
    pub(super) value_ready: &'a BTreeSet<usize>,
    pub(super) unavailable: &'a BTreeSet<usize>,
    pub(super) value_unavailable: &'a BTreeSet<usize>,
    pub(super) type_parameter_defaults:
        &'a [super::project_local_pass_port::PublishedTypeParameterDefault],
    pub(super) aliases: &'a PublishedAliases,
    pub(super) overlays: &'a [(TypeGroupId, ProjectLocalTypeOverlayDraft)],
}

pub(super) fn lower_product_groups(
    product: &LibraryDeclarationProduct,
    interner: &mut Interner,
    materialized: &[usize],
    augmented_groups: &BTreeMap<usize, TypeGroupId>,
    native_array_groups: [Option<usize>; 2],
    seed: Option<ProductLoweringSeed<'_>>,
) -> ProductLoweringReceipt {
    let mut lowerer = ProductLowerer::new(
        product,
        interner,
        augmented_groups.clone(),
        native_array_groups,
        materialized,
        seed,
    );
    for group in materialized {
        let _ = lowerer.lower_group(*group);
    }
    if lowerer.missing_admissions.is_empty() {
        lowerer.finish_type_parameter_constraints();
    }
    if lowerer.missing_admissions.is_empty() {
        lowerer.finish_type_parameter_defaults();
    }
    if lowerer.missing_admissions.is_empty() {
        lowerer.finish_structural_groups(materialized);
    }
    if lowerer.missing_admissions.is_empty() {
        for group in materialized {
            let _ = lowerer.lower_value_group(*group);
        }
    }
    if !lowerer.missing_admissions.is_empty() {
        lowerer.abandon_retry_reservations();
    }
    lowerer.finish_default_application_records();
    let library_record_events = lowerer.records.events();
    let validation_events = lowerer.validations.events();
    let aliases = lowerer.published_aliases();
    ProductLoweringReceipt {
        group_types: lowerer.group_types,
        group_value_types: lowerer.group_value_types,
        ready: lowerer
            .states
            .iter()
            .enumerate()
            .filter_map(|(index, state)| (*state == GroupState::Ready).then_some(index))
            .collect(),
        value_ready: lowerer
            .value_states
            .iter()
            .enumerate()
            .filter_map(|(index, state)| (*state == GroupState::Ready).then_some(index))
            .collect(),
        unavailable: lowerer
            .states
            .iter()
            .enumerate()
            .filter_map(|(index, state)| (*state == GroupState::Unavailable).then_some(index))
            .collect(),
        value_unavailable: lowerer
            .value_states
            .iter()
            .enumerate()
            .filter_map(|(index, state)| (*state == GroupState::Unavailable).then_some(index))
            .collect(),
        type_parameter_ids: lowerer.type_parameter_ids,
        type_parameter_defaults: lowerer.type_parameter_defaults,
        aliases,
        overlays: lowerer.overlays,
        missing_admissions: lowerer.missing_admissions,
        library_record_events,
        validation_events,
    }
}

struct ProductLowerer<'product, 'interner> {
    product: &'product LibraryDeclarationProduct,
    interner: &'interner mut Interner,
    states: Vec<GroupState>,
    group_types: Vec<Option<TypeId>>,
    value_states: Vec<GroupState>,
    group_value_types: Vec<Option<TypeId>>,
    group_parameters: Vec<Vec<TypeParameterKey>>,
    type_parameter_ids: Vec<TypeParamId>,
    type_parameter_types: Vec<TypeId>,
    intrinsic_declarations: BTreeMap<usize, IntrinsicMarker>,
    augmented_groups: BTreeMap<usize, TypeGroupId>,
    staged_objects: BTreeMap<usize, ObjectType>,
    structural_drafts: BTreeMap<usize, ProductStructuralDraft>,
    structural_finalizing: BTreeSet<usize>,
    structural_finalized: BTreeSet<usize>,
    structural_incomplete_groups: BTreeSet<usize>,
    recursive_alias_unavailable_groups: BTreeSet<usize>,
    overlays: Vec<(TypeGroupId, ProjectLocalTypeOverlayDraft)>,
    contextual_type_parameters: Vec<Option<TypeParameterKey>>,
    type_parameter_defaults: Vec<super::project_local_pass_port::PublishedTypeParameterDefault>,
    native_array_groups: [Option<usize>; 2],
    admitted_groups: BTreeSet<usize>,
    admitted_parameters: BTreeSet<usize>,
    parameter_groups: Vec<BTreeSet<usize>>,
    group_scc_members: Vec<Vec<usize>>,
    missing_admissions: BTreeSet<usize>,
    owned_object_reservations: BTreeSet<TypeId>,
    alias_ids: Vec<Option<AliasId>>,
    alias_templates: Vec<Option<TypeId>>,
    alias_bodies: Vec<Option<TypeId>>,
    alias_unavailable: Vec<Option<PublishedAliasUnavailable>>,
    alias_indirection_depth: u32,
    resolving_alias_depth: Vec<Option<u32>>,
    seed_aliases: PublishedAliases,
    records: LibraryRecordAccumulator,
    validations: ProjectLocalValidationAccumulator,
    current_validation_group: Option<usize>,
    type_parameter_owners: Vec<Option<IssueOwner>>,
    default_applications: BTreeMap<(usize, LibraryRecordOwnerIdentity), PendingDefaultApplication>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IssueOwner {
    identity: LibraryRecordOwnerIdentity,
    start: u32,
}

#[derive(Clone, Copy, Debug)]
struct PendingDefaultApplication {
    group: usize,
    supplied: usize,
    owner: IssueOwner,
}

#[derive(Clone)]
struct ProductStructuralDraft {
    own: ObjectType,
    parents: Vec<ProductHeritageDraft>,
}

#[derive(Clone)]
struct ProductHeritageDraft {
    recipe: TypeRecipeKey,
    ty: TypeId,
}

fn remember_lowering_error<T>(
    first: &mut Option<ProductLoweringError>,
    result: Result<T, ProductLoweringError>,
) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            if first.is_none() {
                *first = Some(error);
            }
            None
        }
    }
}

impl<'product, 'interner> ProductLowerer<'product, 'interner> {
    fn new(
        product: &'product LibraryDeclarationProduct,
        interner: &'interner mut Interner,
        augmented_groups: BTreeMap<usize, TypeGroupId>,
        native_array_groups: [Option<usize>; 2],
        materialized: &[usize],
        seed: Option<ProductLoweringSeed<'_>>,
    ) -> Self {
        let mut next_type_parameter = 0u32;
        let mut type_parameter_ids = Vec::with_capacity(product.type_parameters().len());
        let mut type_parameter_types = Vec::with_capacity(product.type_parameters().len());
        for parameter in product.type_parameters() {
            let id = TypeParamId(next_type_parameter);
            next_type_parameter = next_type_parameter
                .checked_add(1)
                .expect("product type-parameter identity overflow");
            type_parameter_ids.push(id);
            type_parameter_types.push(
                interner.intern_type_param(id, product.strings()[parameter.name.index()].clone()),
            );
        }
        let intrinsic_declarations: BTreeMap<usize, IntrinsicMarker> = product
            .intrinsic_marker_rows()
            .iter()
            .map(|row| (row.declaration.index(), row.marker))
            .collect();
        let contextual_type_parameters = build_contextual_type_parameter_index(product);
        let group_parameters = product
            .groups()
            .iter()
            .map(|group| {
                group
                    .fragments
                    .iter()
                    .find_map(|declaration| {
                        match &product.declarations()[declaration.index()].payload {
                            DeclarationPayload::Class {
                                type_parameters, ..
                            }
                            | DeclarationPayload::Interface {
                                type_parameters, ..
                            }
                            | DeclarationPayload::TypeAlias {
                                type_parameters, ..
                            } => Some(type_parameters.to_vec()),
                            _ => None,
                        }
                    })
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        let mut type_parameter_owners = vec![None; product.type_parameters().len()];
        let mut parameter_groups = vec![BTreeSet::new(); product.type_parameters().len()];
        for (group_index, group) in product.groups().iter().enumerate() {
            for declaration in group.fragments.iter().copied() {
                let row = &product.declarations()[declaration.index()];
                let parameters = match &row.payload {
                    DeclarationPayload::Class {
                        type_parameters, ..
                    }
                    | DeclarationPayload::Interface {
                        type_parameters, ..
                    }
                    | DeclarationPayload::TypeAlias {
                        type_parameters, ..
                    } => type_parameters,
                    _ => continue,
                };
                let owner = IssueOwner {
                    identity: LibraryRecordOwnerIdentity::Declaration(declaration.index()),
                    start: row.binding_site.start,
                };
                for parameter in parameters.iter().copied() {
                    type_parameter_owners[parameter.index()].get_or_insert(owner);
                    parameter_groups[parameter.index()].insert(group_index);
                }
            }
        }
        let admitted_groups = materialized.iter().copied().collect::<BTreeSet<_>>();
        let admitted_parameters = parameter_groups
            .iter()
            .enumerate()
            .filter_map(|(parameter, groups)| {
                groups
                    .iter()
                    .any(|group| admitted_groups.contains(group))
                    .then_some(parameter)
            })
            .collect();
        let mut group_scc_members = vec![Vec::new(); product.group_count()];
        for component in product.dependency_sccs() {
            let members = component
                .into_iter()
                .map(|group| group.index())
                .collect::<Vec<_>>();
            for member in &members {
                group_scc_members[*member] = members.clone();
            }
        }
        let seeded_group_types = seed
            .as_ref()
            .map(|seed| seed.group_types.to_vec())
            .unwrap_or_else(|| (0..product.group_count()).map(|_| None).collect());
        let seeded_group_value_types = seed
            .as_ref()
            .map(|seed| seed.group_value_types.to_vec())
            .unwrap_or_else(|| (0..product.group_count()).map(|_| None).collect());
        let seed_aliases = seed
            .as_ref()
            .map(|seed| seed.aliases.clone())
            .unwrap_or_else(PublishedAliases::empty);
        debug_assert_eq!(seeded_group_types.len(), product.group_count());
        debug_assert_eq!(seeded_group_value_types.len(), product.group_count());
        let mut alias_ids = vec![None; product.group_count()];
        let mut alias_templates = vec![None; product.group_count()];
        for &index in materialized {
            let alias_declaration = product.groups()[index]
                .fragments
                .iter()
                .find(|declaration| {
                    matches!(
                        product.declarations()[declaration.index()].payload,
                        DeclarationPayload::TypeAlias { .. }
                    ) && !intrinsic_declarations.contains_key(&declaration.index())
                });
            if alias_declaration.is_none() {
                continue;
            }
            if let Some(template) = seeded_group_types[index]
                .filter(|template| interner.store().tag(*template) == TypeTag::AliasTemplate)
            {
                alias_ids[index] = interner.store().alias_template_id(template);
                alias_templates[index] = Some(template);
                continue;
            }
            let name = product.strings()[product.groups()[index].name.index()].clone();
            let (alias, template) = interner.reserve_alias_template(name);
            alias_ids[index] = Some(alias);
            alias_templates[index] = Some(template);
        }
        let mut lowerer = Self {
            product,
            interner,
            states: (0..product.group_count())
                .map(|index| {
                    if seed
                        .as_ref()
                        .is_some_and(|seed| seed.unavailable.contains(&index))
                    {
                        GroupState::Unavailable
                    } else if seed
                        .as_ref()
                        .is_some_and(|seed| seed.ready.contains(&index))
                    {
                        GroupState::Ready
                    } else {
                        GroupState::Pending
                    }
                })
                .collect(),
            group_types: seeded_group_types,
            value_states: (0..product.group_count())
                .map(|index| {
                    if seed
                        .as_ref()
                        .is_some_and(|seed| seed.value_unavailable.contains(&index))
                    {
                        GroupState::Unavailable
                    } else if seed
                        .as_ref()
                        .is_some_and(|seed| seed.value_ready.contains(&index))
                    {
                        GroupState::Ready
                    } else {
                        GroupState::Pending
                    }
                })
                .collect(),
            group_value_types: seeded_group_value_types,
            group_parameters,
            type_parameter_ids,
            type_parameter_types,
            intrinsic_declarations,
            augmented_groups,
            staged_objects: BTreeMap::new(),
            structural_drafts: BTreeMap::new(),
            structural_finalizing: BTreeSet::new(),
            structural_finalized: BTreeSet::new(),
            structural_incomplete_groups: BTreeSet::new(),
            recursive_alias_unavailable_groups: BTreeSet::new(),
            overlays: seed
                .as_ref()
                .map(|seed| seed.overlays.to_vec())
                .unwrap_or_default(),
            contextual_type_parameters,
            type_parameter_defaults: seed
                .as_ref()
                .map(|seed| seed.type_parameter_defaults.to_vec())
                .unwrap_or_else(|| {
                    product
                        .type_parameters()
                        .iter()
                        .map(|parameter| {
                            if parameter.default.is_some() {
                                super::project_local_pass_port::PublishedTypeParameterDefault::Unsupported
                            } else {
                                super::project_local_pass_port::PublishedTypeParameterDefault::Absent
                            }
                        })
                        .collect()
                }),
            native_array_groups,
            admitted_groups,
            admitted_parameters,
            parameter_groups,
            group_scc_members,
            missing_admissions: BTreeSet::new(),
            owned_object_reservations: BTreeSet::new(),
            alias_ids,
            alias_templates,
            alias_bodies: vec![None; product.group_count()],
            alias_unavailable: vec![None; product.group_count()],
            alias_indirection_depth: 0,
            resolving_alias_depth: vec![None; product.group_count()],
            seed_aliases,
            records: LibraryRecordAccumulator::default(),
            validations: ProjectLocalValidationAccumulator::default(),
            current_validation_group: None,
            type_parameter_owners,
            default_applications: BTreeMap::new(),
        };
        lowerer.reserve_recursive_groups();
        lowerer
    }

    fn published_aliases(&self) -> PublishedAliases {
        let mut terminals = self
            .seed_aliases
            .owned_terminals()
            .into_iter()
            .collect::<FxHashMap<_, _>>();
        for group in 0..self.alias_ids.len() {
            let (Some(alias), Some(template)) =
                (self.alias_ids[group], self.alias_templates[group])
            else {
                continue;
            };
            let terminal = if let Some(unavailable) = self.alias_unavailable[group] {
                PublishedAliasTerminal::Unavailable(unavailable)
            } else if let Some(body) = self.alias_bodies[group] {
                let type_params = self.group_parameters[group]
                    .iter()
                    .map(|parameter| self.type_parameter_ids[parameter.index()])
                    .collect();
                PublishedAliasTerminal::Ready(PublishedAliasSurface::new(
                    alias,
                    template,
                    type_params,
                    body,
                ))
            } else if let Some(existing) = self.product_alias_terminal_from_seed(alias) {
                existing
            } else {
                PublishedAliasTerminal::Unavailable(PublishedAliasUnavailable::ComponentFailure)
            };
            terminals.insert(alias, terminal);
        }
        PublishedAliases::from_terminals(terminals)
            .expect("product alias terminals have exact alias identities")
    }

    fn declaration_owner(&self, declaration: DeclarationKey) -> IssueOwner {
        IssueOwner {
            identity: LibraryRecordOwnerIdentity::Declaration(declaration.index()),
            start: self.product.declarations()[declaration.index()]
                .binding_site
                .start,
        }
    }

    fn member_owner(&self, member: MemberKey) -> IssueOwner {
        IssueOwner {
            identity: LibraryRecordOwnerIdentity::Member(member.index()),
            start: self.product.members()[member.index()].site.start,
        }
    }

    fn heritage_owner(&self, recipe: TypeRecipeKey) -> IssueOwner {
        IssueOwner {
            identity: LibraryRecordOwnerIdentity::Heritage(recipe.index()),
            start: self.product.types()[recipe.index()].site.start,
        }
    }

    fn type_parameter_owner(&self, parameter: usize) -> IssueOwner {
        self.type_parameter_owners[parameter]
            .expect("admitted declaration type parameter has an authenticated owner")
    }

    fn abandon_retry_reservations(&mut self) {
        let reservations = std::mem::take(&mut self.owned_object_reservations);
        for reservation in &reservations {
            let _ = self.interner.abandon_reserved_object(*reservation);
            for group in 0..self.group_types.len() {
                if self.group_types[group] == Some(*reservation) {
                    self.group_types[group] = None;
                    self.states[group] = GroupState::Pending;
                    self.structural_drafts.remove(&group);
                    self.staged_objects.remove(&group);
                    self.structural_finalizing.remove(&group);
                    self.structural_finalized.remove(&group);
                }
            }
        }
        self.overlays
            .retain(|(_, overlay)| !reservations.contains(&overlay.template));
    }

    fn product_alias_terminal_from_seed(&self, alias: AliasId) -> Option<PublishedAliasTerminal> {
        self.seed_aliases.terminal(alias).cloned()
    }

    fn lower_value_group(
        &mut self,
        group_index: usize,
    ) -> Result<Option<TypeId>, ProductLoweringError> {
        match self.value_states[group_index] {
            GroupState::Ready => return Ok(self.group_value_types[group_index]),
            GroupState::Unavailable => {
                return Err(
                    if self
                        .recursive_alias_unavailable_groups
                        .contains(&group_index)
                    {
                        ProductLoweringError::RecursiveAliasWithoutReservation
                    } else {
                        ProductLoweringError::UnsupportedRecipe
                    },
                );
            }
            GroupState::Resolving => return Ok(self.group_value_types[group_index]),
            GroupState::Pending => {}
        }
        self.value_states[group_index] = GroupState::Resolving;
        let result = self.lower_resolving_value_group(group_index);
        if result.is_err() {
            if let Some(reserved) = self.group_value_types[group_index].take() {
                self.interner
                    .abandon_reserved_object(reserved)
                    .expect("failed namespace value leaves one pending reserved object");
            }
            self.value_states[group_index] = GroupState::Unavailable;
            self.fail_group_component(group_index);
        }
        result
    }

    fn lower_resolving_value_group(
        &mut self,
        group_index: usize,
    ) -> Result<Option<TypeId>, ProductLoweringError> {
        let group = self.product.groups()[group_index].clone();
        if !group.slots.value
            && !group.slots.namespace
            && !group_has_runtime_value(self.product, &group)
        {
            self.value_states[group_index] = GroupState::Ready;
            return Ok(None);
        }
        let mut object = ObjectType::default();
        let mut direct = None;
        if group.slots.namespace {
            self.group_value_types[group_index] = Some(self.interner.reserve_object());
        }
        let compilation_global = self.product.containers()[group.container.index()].kind
            == super::project_local_pass_port::ContainerKind::CompilationGlobal;
        let fragments = self
            .product
            .groups()
            .iter()
            .filter(|candidate| {
                candidate.name == group.name
                    && if compilation_global {
                        self.product.containers()[candidate.container.index()].kind
                            == super::project_local_pass_port::ContainerKind::CompilationGlobal
                    } else {
                        candidate.container == group.container
                    }
            })
            .flat_map(|candidate| candidate.fragments.iter().copied())
            .collect::<Vec<_>>();
        for declaration in fragments {
            let declaration_owner = self.declaration_owner(declaration);
            let declaration = self.product.declarations()[declaration.index()].clone();
            match declaration.payload {
                DeclarationPayload::Variable {
                    annotation: Some(annotation),
                } => {
                    if self
                        .namespace_member_type_syntax_is_unsupported(declaration.region, annotation)
                    {
                        self.value_states[group_index] = GroupState::Unavailable;
                        return self.unsupported(
                            annotation,
                            declaration_owner,
                            "annotation-lower/type-syntax/unsupported",
                            "namespace member type syntax could not be lowered",
                        );
                    }
                    let ty = self.lower_type(annotation, declaration_owner)?;
                    let unavailable_reference = matches!(
                        self.product.types()[annotation.index()].node,
                        TypeRecipeNode::Reference {
                            resolution: TypeReferenceResolution::Group(group),
                            ..
                        } if self.structural_incomplete_groups.contains(&group.index())
                    );
                    if unavailable_reference {
                        self.value_states[group_index] = GroupState::Unavailable;
                        return self.unsupported(
                            annotation,
                            declaration_owner,
                            "annotation-lower/type-reference/unavailable",
                            "referenced type group is unavailable",
                        );
                    }
                    direct = Some(ty);
                }
                DeclarationPayload::Function { signature } => {
                    object
                        .call_signatures
                        .push(self.lower_signature(signature, declaration_owner)?);
                }
                DeclarationPayload::Class { members, .. } => {
                    let mut class_error = None;
                    if object
                        .properties
                        .iter()
                        .all(|property| property.key.as_string() != Some("prototype"))
                    {
                        let instance = remember_lowering_error(
                            &mut class_error,
                            self.lower_group(group_index),
                        )
                        .flatten();
                        if let Some(instance) = instance {
                            object.properties.push(PropertyType {
                                key: PropertyKey::String("prototype".to_owned()),
                                ty: instance,
                                write_ty: None,
                                optional: false,
                                visibility: Visibility::Public,
                                declaring_class: None,
                                readonly: false,
                                is_accessor: false,
                            });
                        } else if class_error.is_none() {
                            let _ = remember_lowering_error(
                                &mut class_error,
                                self.unsupported_group::<()>(
                                    group_index,
                                    "annotation-lower/class/value-instance",
                                    "class value has no instance type",
                                ),
                            );
                        }
                    }
                    for member in members.iter() {
                        let key = *member;
                        let member = self.product.members()[key.index()].clone();
                        let result = if member.kind == MemberKind::Constructor {
                            let owner = self.member_owner(key);
                            self.required_member_signature(key, member.signature)
                                .and_then(|signature| {
                                    self.with_indirection(|lowerer| {
                                        lowerer.lower_signature(signature, owner)
                                    })
                                })
                                .map(|signature| object.construct_signatures.push(signature))
                        } else if member.static_member {
                            self.lower_member(key, &member, &mut object, true)
                        } else {
                            Ok(())
                        };
                        let _ = remember_lowering_error(&mut class_error, result);
                    }
                    if let Some(error) = class_error {
                        return Err(error);
                    }
                }
                DeclarationPayload::Enum { members } => {
                    for member in members.iter() {
                        let member_key = *member;
                        let member = self.product.members()[member_key.index()].clone();
                        let Some(name) = member.name.as_ref() else {
                            let _ = self.unsupported_member::<()>(
                                member_key,
                                "annotation-lower/enum-member/missing-name",
                                "enum member has no name",
                            );
                            continue;
                        };
                        let Some(key) = property_key(self.product, name) else {
                            let _ = self.unsupported_member::<()>(
                                member_key,
                                "annotation-lower/enum-member/property-key",
                                "enum member property key not modeled",
                            );
                            continue;
                        };
                        object.properties.push(PropertyType {
                            key,
                            ty: self.interner.well_known().number,
                            write_ty: None,
                            optional: false,
                            visibility: Visibility::Public,
                            declaring_class: None,
                            readonly: true,
                            is_accessor: false,
                        });
                    }
                }
                DeclarationPayload::Namespace { child_region }
                | DeclarationPayload::Global { child_region } => {
                    let child_container = self.product.regions()[child_region.index()].container;
                    let children = self
                        .product
                        .groups()
                        .iter()
                        .enumerate()
                        .filter_map(|(index, child)| {
                            (child.container == child_container
                                && (child.slots.namespace
                                    || group_has_runtime_value(self.product, child)))
                            .then_some(index)
                        })
                        .collect::<Vec<_>>();
                    for child in children {
                        let Some(ty) = self.lower_value_group(child)? else {
                            continue;
                        };
                        object.properties.push(PropertyType {
                            key: PropertyKey::String(
                                self.product.strings()[self.product.groups()[child].name.index()]
                                    .clone(),
                            ),
                            ty,
                            write_ty: None,
                            optional: false,
                            visibility: Visibility::Public,
                            declaring_class: None,
                            readonly: true,
                            is_accessor: false,
                        });
                    }
                }
                DeclarationPayload::Variable { annotation: None }
                | DeclarationPayload::Interface { .. }
                | DeclarationPayload::TypeAlias { .. }
                | DeclarationPayload::Import
                | DeclarationPayload::ImportEquals
                | DeclarationPayload::NamespaceExport
                | DeclarationPayload::Parameter { .. }
                | DeclarationPayload::CatchParameter { .. }
                | DeclarationPayload::DefaultExportExpression => {}
            }
        }
        if let Some(ty) = direct {
            if self.interner.store().function_type(ty).is_some() {
                object.call_signatures.push(ty);
            } else if let Some(source) = self.interner.store().object_type(ty).cloned() {
                object.properties.extend(source.properties);
                object.string_index = source.string_index;
                object.number_index = source.number_index;
                object.call_signatures.extend(source.call_signatures);
                object
                    .construct_signatures
                    .extend(source.construct_signatures);
            } else if object.properties.is_empty()
                && object.call_signatures.is_empty()
                && object.construct_signatures.is_empty()
            {
                self.group_value_types[group_index] = Some(ty);
                self.value_states[group_index] = GroupState::Ready;
                return Ok(Some(ty));
            }
        }
        let ty = if let Some(reserved) = self.group_value_types[group_index] {
            self.interner.fill_object(reserved, object);
            reserved
        } else {
            self.interner.intern_object(object)
        };
        self.group_value_types[group_index] = Some(ty);
        self.value_states[group_index] = GroupState::Ready;
        Ok(Some(ty))
    }

    fn namespace_member_type_syntax_is_unsupported(
        &self,
        region: super::project_local_pass_port::RegionKey,
        annotation: TypeRecipeKey,
    ) -> bool {
        let container = self.product.regions()[region.index()].container;
        if self.product.containers()[container.index()].kind
            != super::project_local_pass_port::ContainerKind::Namespace
        {
            return false;
        }
        let TypeRecipeNode::Object(members) = &self.product.types()[annotation.index()].node else {
            return false;
        };
        members.iter().any(|member| {
            let member = &self.product.members()[member.index()];
            matches!(member.kind, MemberKind::Construct | MemberKind::Constructor)
                && member.signature.is_some_and(|signature| {
                    self.product.signatures()[signature.index()]
                        .type_parameters
                        .iter()
                        .any(|parameter| {
                            self.product.type_parameters()[parameter.index()]
                                .default
                                .is_some()
                        })
                })
        })
    }

    fn reserve_recursive_groups(&mut self) {
        for (index, group) in self.product.groups().iter().enumerate() {
            if !self.admitted_groups.contains(&index)
                || self.states[index] != GroupState::Pending
                || !group.slots.r#type
            {
                continue;
            }
            if let Some(template) = self.alias_templates[index] {
                self.group_types[index] = Some(template);
                continue;
            }
            let reservation = group.fragments.iter().find_map(|declaration| {
                let declaration = &self.product.declarations()[declaration.index()];
                match &declaration.payload {
                    DeclarationPayload::Class { .. } | DeclarationPayload::Interface { .. } => {
                        Some(self.interner.reserve_object())
                    }
                    DeclarationPayload::TypeAlias { .. } => None,
                    _ => None,
                }
            });
            if let Some(reservation) = reservation {
                self.owned_object_reservations.insert(reservation);
            }
            self.group_types[index] = reservation;
        }
    }

    fn lower_group(&mut self, group_index: usize) -> Result<Option<TypeId>, ProductLoweringError> {
        if !self.admitted_groups.contains(&group_index) {
            self.missing_admissions.insert(group_index);
            return Err(ProductLoweringError::MissingAdmission(group_index));
        }
        match self.states[group_index] {
            GroupState::Ready => return Ok(self.group_types[group_index]),
            GroupState::Unavailable => {
                return Err(
                    if self
                        .recursive_alias_unavailable_groups
                        .contains(&group_index)
                    {
                        ProductLoweringError::RecursiveAliasWithoutReservation
                    } else {
                        ProductLoweringError::UnsupportedRecipe
                    },
                );
            }
            GroupState::Resolving => {
                let structural =
                    self.product.groups()[group_index]
                        .fragments
                        .iter()
                        .any(|declaration| {
                            matches!(
                                self.product.declarations()[declaration.index()].payload,
                                DeclarationPayload::Class { .. }
                                    | DeclarationPayload::Interface { .. }
                            )
                        });
                if structural {
                    return Ok(self.group_types[group_index]);
                }
                if self.alias_indirection_depth
                    > self.resolving_alias_depth[group_index].unwrap_or(u32::MAX)
                {
                    if let Some(template) = self.alias_templates[group_index] {
                        return Ok(Some(template));
                    }
                }
                return Err(ProductLoweringError::RecursiveAliasWithoutReservation);
            }
            GroupState::Pending => {}
        }
        self.states[group_index] = GroupState::Resolving;
        self.resolving_alias_depth[group_index] =
            self.alias_templates[group_index].map(|_| self.alias_indirection_depth);
        let group = &self.product.groups()[group_index];
        if !group.slots.r#type {
            self.states[group_index] = GroupState::Ready;
            return Ok(None);
        }
        let previous_validation_group = self.current_validation_group.replace(group_index);
        let result = self.lower_group_fragments(group_index, &group.fragments);
        self.current_validation_group = previous_validation_group;
        match result {
            Ok(ty) => {
                self.resolving_alias_depth[group_index] = None;
                self.group_types[group_index] = Some(ty);
                if !self.structural_drafts.contains_key(&group_index) {
                    self.states[group_index] = GroupState::Ready;
                }
                Ok(Some(ty))
            }
            Err(error) => {
                self.fail_group_component(group_index);
                if error == ProductLoweringError::RecursiveAliasWithoutReservation {
                    self.recursive_alias_unavailable_groups.insert(group_index);
                    if self.alias_ids[group_index].is_some() {
                        self.alias_unavailable[group_index] =
                            Some(PublishedAliasUnavailable::DirectCycle);
                    }
                }
                Err(error)
            }
        }
    }

    fn lower_group_fragments(
        &mut self,
        group_index: usize,
        fragments: &[DeclarationKey],
    ) -> Result<TypeId, ProductLoweringError> {
        if let Some((declaration, marker)) = fragments.iter().find_map(|declaration| {
            self.intrinsic_declarations
                .get(&declaration.index())
                .copied()
                .map(|marker| (*declaration, marker))
        }) {
            return self.lower_intrinsic_marker(declaration, marker);
        }
        let structural = fragments.iter().any(|declaration| {
            matches!(
                self.product.declarations()[declaration.index()].payload,
                DeclarationPayload::Class { .. } | DeclarationPayload::Interface { .. }
            )
        });
        if structural {
            let mut own = ObjectType::default();
            let mut parents = Vec::new();
            let mut unavailable_member_keys = Vec::new();
            let mut omitted_overload_keys = Vec::new();
            let mut structural_error = None;
            for declaration in fragments {
                let payload = self.product.declarations()[declaration.index()]
                    .payload
                    .clone();
                let (type_parameters, members, heritage, class_members) = match payload {
                    DeclarationPayload::Class {
                        type_parameters,
                        members,
                        heritage,
                    } => (type_parameters, members, heritage, true),
                    DeclarationPayload::Interface {
                        type_parameters,
                        members,
                        heritage,
                    } => (type_parameters, members, heritage, false),
                    _ => continue,
                };
                let mut fragment = ObjectType::default();
                let members_ready = remember_lowering_error(
                    &mut structural_error,
                    self.lower_members(
                        Some(group_index),
                        &members,
                        &mut fragment,
                        class_members,
                        &mut omitted_overload_keys,
                    ),
                )
                .is_some();
                if members_ready {
                    if let Some(fragment) = remember_lowering_error(
                        &mut structural_error,
                        self.canonicalize_fragment_object(group_index, &type_parameters, fragment),
                    ) {
                        unavailable_member_keys.extend(
                            fragment
                                .properties
                                .iter()
                                .filter(|property| property.ty == self.interner.well_known().error)
                                .map(|property| property.key.clone()),
                        );
                        merge_object_surface(&mut own, fragment);
                    }
                }
                for parent in heritage {
                    let owner = self.heritage_owner(parent);
                    match self.lower_type(parent, owner) {
                        Ok(parent_type) if parent_type != self.interner.well_known().error => {
                            if let Some(parent_type) = remember_lowering_error(
                                &mut structural_error,
                                self.canonicalize_fragment_type(
                                    group_index,
                                    &type_parameters,
                                    parent_type,
                                ),
                            ) {
                                parents.push(ProductHeritageDraft {
                                    recipe: parent,
                                    ty: parent_type,
                                });
                            }
                        }
                        Ok(_) => {
                            self.structural_incomplete_groups.insert(group_index);
                            if structural_error.is_none() {
                                structural_error = Some(ProductLoweringError::UnsupportedRecipe);
                            }
                        }
                        Err(error) => {
                            self.structural_incomplete_groups.insert(group_index);
                            if structural_error.is_none() {
                                structural_error = Some(error);
                            }
                        }
                    }
                }
            }
            if let Some(error) = structural_error {
                return Err(error);
            }
            own.properties.retain(|property| {
                !unavailable_member_keys.contains(&property.key)
                    && !omitted_overload_keys.contains(&property.key)
            });
            self.stage_heritage_validation(group_index, &parents, &own);
            let reserved = self.group_types[group_index]
                .ok_or(ProductLoweringError::RecursiveAliasWithoutReservation)?;
            self.structural_drafts
                .insert(group_index, ProductStructuralDraft { own, parents });
            return Ok(reserved);
        }
        for declaration in fragments {
            if let DeclarationPayload::TypeAlias { value, .. } =
                self.product.declarations()[declaration.index()].payload
            {
                return self.lower_alias(group_index, value, self.declaration_owner(*declaration));
            }
        }
        self.unsupported_group(
            group_index,
            "annotation-lower/declaration-group/unsupported",
            "declaration group has no modeled type producer",
        )
    }

    fn canonicalize_fragment_object(
        &mut self,
        group_index: usize,
        parameters: &[TypeParameterKey],
        object: ObjectType,
    ) -> Result<ObjectType, ProductLoweringError> {
        if parameters == self.group_parameters[group_index] {
            return Ok(object);
        }
        let ty = self.interner.intern_object(object);
        let ty = self.canonicalize_fragment_type(group_index, parameters, ty)?;
        let lowered = self.interner.store().object_type(ty).cloned();
        lowered.map_or_else(
            || {
                self.unsupported_group(
                    group_index,
                    "annotation-lower/merged-fragment/object-shape",
                    "merged fragment did not lower to an object",
                )
            },
            Ok,
        )
    }

    fn canonicalize_fragment_type(
        &mut self,
        group_index: usize,
        parameters: &[TypeParameterKey],
        ty: TypeId,
    ) -> Result<TypeId, ProductLoweringError> {
        let canonical = &self.group_parameters[group_index];
        if parameters == canonical {
            return Ok(ty);
        }
        if parameters.len() != canonical.len() {
            return self.unsupported_group(
                group_index,
                "annotation-lower/merged-fragment/type-parameters",
                "merged fragment type-parameter arity differs",
            );
        }
        let substitutions = parameters
            .iter()
            .zip(canonical)
            .map(|(from, to)| {
                (
                    self.type_parameter_ids[from.index()],
                    self.type_parameter_types[to.index()],
                )
            })
            .collect();
        Ok(substitute(self.interner, ty, &substitutions))
    }

    fn object_type_or_staged(&self, ty: TypeId) -> Option<ObjectType> {
        self.interner.store().object_type(ty).cloned().or_else(|| {
            self.group_types
                .iter()
                .position(|candidate| *candidate == Some(ty))
                .and_then(|group| self.staged_objects.get(&group).cloned())
        })
    }

    fn validation_site(&self, site: SourceSite, owner_start: u32) -> ProjectLocalValidationSite {
        let ordinal = self.product.sources()[site.source.index()].ordinal;
        ProjectLocalValidationSite {
            file_ordinal: super::project_local_pass_port::LibraryFileOrdinal::new(
                usize::try_from(ordinal).expect("library source ordinal fits usize"),
            ),
            owner_start,
            site,
        }
    }

    fn stage_heritage_validation(
        &mut self,
        group: usize,
        parents: &[ProductHeritageDraft],
        own: &ObjectType,
    ) {
        let Some(declaration) =
            self.product.groups()[group]
                .fragments
                .iter()
                .copied()
                .find(|declaration| {
                    matches!(
                        self.product.declarations()[declaration.index()].payload,
                        DeclarationPayload::Interface { .. }
                    )
                })
        else {
            return;
        };
        let declaration_row = &self.product.declarations()[declaration.index()];
        let site = self.validation_site(
            declaration_row.binding_site,
            declaration_row.binding_site.start,
        );
        let derived_name =
            self.product.strings()[self.product.groups()[group].name.index()].clone();
        let bases = parents
            .iter()
            .filter_map(|parent| {
                let TypeRecipeNode::Reference {
                    resolution: TypeReferenceResolution::Group(base_group),
                    path,
                    ..
                } = &self.product.types()[parent.recipe.index()].node
                else {
                    return None;
                };
                let application = self
                    .interner
                    .store()
                    .instantiation_type(parent.ty)
                    .map(|application| application.args.clone())
                    .unwrap_or_default();
                let name = path
                    .last()
                    .map(|name| self.product.strings()[name.index()].clone())
                    .unwrap_or_else(|| "unknown".to_owned());
                Some(ProjectLocalHeritageBase {
                    group: base_group.index(),
                    application,
                    name,
                })
            })
            .collect::<Vec<_>>();
        if bases.is_empty() {
            return;
        }
        self.validations.stage(ProjectLocalValidationEvent {
            producer: ProjectLocalValidationProducer::Heritage { group },
            candidate: ProjectLocalValidationCandidate::Heritage(ProjectLocalHeritageValidation {
                group,
                site,
                own: own.clone(),
                bases,
                derived_name,
            }),
        });
    }

    fn qualified_validation_owner(
        &self,
        recipe: TypeRecipeKey,
        owner: IssueOwner,
    ) -> Option<(usize, IssueOwner)> {
        let (declaration, diagnostic_owner) = match owner.identity {
            LibraryRecordOwnerIdentity::Declaration(declaration) => {
                let declaration = self.product.declaration_key_at(declaration)?;
                (declaration, owner)
            }
            LibraryRecordOwnerIdentity::Member(member) => {
                let declaration = self.product.declarations().iter().enumerate().find_map(
                    |(index, declaration)| {
                        let members = match &declaration.payload {
                            DeclarationPayload::Class { members, .. }
                            | DeclarationPayload::Interface { members, .. } => members,
                            _ => return None,
                        };
                        members
                            .iter()
                            .any(|candidate| candidate.index() == member)
                            .then(|| {
                                self.product
                                    .declaration_key_at(index)
                                    .expect("declaration key exists")
                            })
                    },
                )?;
                (declaration, owner)
            }
            LibraryRecordOwnerIdentity::Heritage(heritage) => {
                if heritage != recipe.index() {
                    return None;
                }
                let declaration = self.product.declarations().iter().enumerate().find_map(
                    |(index, declaration)| {
                        let heritage = match &declaration.payload {
                            DeclarationPayload::Class { heritage, .. }
                            | DeclarationPayload::Interface { heritage, .. } => heritage,
                            _ => return None,
                        };
                        heritage.contains(&recipe).then(|| {
                            self.product
                                .declaration_key_at(index)
                                .expect("declaration key exists")
                        })
                    },
                )?;
                (declaration, self.declaration_owner(declaration))
            }
        };
        self.product
            .groups()
            .iter()
            .position(|group| group.fragments.contains(&declaration))
            .map(|group| (group, diagnostic_owner))
    }

    fn stage_qualified_path_validation(&mut self, recipe: TypeRecipeKey, owner: IssueOwner) {
        let Some((group, diagnostic_owner)) = self.qualified_validation_owner(recipe, owner) else {
            return;
        };
        let Some(path) = self
            .product
            .qualified_path_rows()
            .iter()
            .find(|path| path.recipe == recipe)
            .copied()
        else {
            return;
        };
        let QualifiedPathResolutionOutcome::Unresolved {
            cause: QualifiedPathUnresolvedCause::MissingRoot,
            failing_segment_ordinal,
        } = path.resolution
        else {
            return;
        };
        let Some(segment) = self
            .product
            .qualified_path_segment_rows()
            .iter()
            .find(|segment| segment.recipe == recipe && segment.ordinal == failing_segment_ordinal)
            .copied()
        else {
            return;
        };
        let segment_count = self
            .product
            .qualified_path_segment_rows()
            .iter()
            .filter(|segment| segment.recipe == recipe)
            .count();
        let kind = match (path.role, segment_count) {
            (QualifiedPathRole::Reference, 2..) => {
                ProjectLocalQualifiedPathDiagnosticKind::MissingNamespace
            }
            (QualifiedPathRole::Reference | QualifiedPathRole::Query, _) => {
                ProjectLocalQualifiedPathDiagnosticKind::MissingName
            }
            (QualifiedPathRole::ImportQualifier, _) => return,
        };
        self.validations.stage(ProjectLocalValidationEvent {
            producer: ProjectLocalValidationProducer::QualifiedPath {
                recipe: recipe.index(),
            },
            candidate: ProjectLocalValidationCandidate::QualifiedPath(
                ProjectLocalQualifiedPathValidation {
                    group,
                    site: self.validation_site(segment.site, diagnostic_owner.start),
                    name: self.product.strings()[segment.name.index()].clone(),
                    kind,
                },
            ),
        });
    }

    fn finish_structural_groups(&mut self, materialized: &[usize]) {
        for group in materialized {
            self.finish_structural_group(*group);
        }
    }

    fn finish_structural_group(&mut self, group: usize) {
        if self.states[group] == GroupState::Unavailable
            || self.structural_finalized.contains(&group)
            || !self.structural_drafts.contains_key(&group)
            || !self.structural_finalizing.insert(group)
        {
            return;
        }
        let draft = self.structural_drafts[&group].clone();
        for parent in &draft.parents {
            let base = self
                .interner
                .store()
                .instantiation_type(parent.ty)
                .map_or(parent.ty, |application| application.base);
            if let Some(parent_group) = self
                .group_types
                .iter()
                .position(|candidate| *candidate == Some(base))
            {
                self.finish_structural_group(parent_group);
                if self.states[parent_group] == GroupState::Unavailable {
                    self.structural_finalizing.remove(&group);
                    self.fail_group_component(group);
                    return;
                }
            }
        }
        let mut object = draft.own.clone();
        let Some(reserved) = self.group_types[group] else {
            self.structural_finalizing.remove(&group);
            self.fail_group_component(group);
            return;
        };
        for parent in &draft.parents {
            let Some(parent_object) = self.materialize_object_type(parent.ty) else {
                self.structural_finalizing.remove(&group);
                self.fail_group_component(group);
                return;
            };
            merge_object_surface(&mut object, parent_object);
        }
        self.staged_objects.insert(group, object.clone());
        if let Some(local_group) = self.augmented_groups.get(&group).copied() {
            let parameters = self.group_parameters[group]
                .iter()
                .map(|parameter| self.type_parameter_ids[parameter.index()])
                .collect::<Vec<_>>();
            let parameter_names = self.group_parameters[group]
                .iter()
                .map(|parameter| {
                    self.product.strings()[self.product.type_parameters()[parameter.index()]
                        .name
                        .index()]
                    .clone()
                })
                .collect::<Vec<_>>();
            self.overlays.push((
                local_group,
                ProjectLocalTypeOverlayDraft {
                    template: reserved,
                    parameter_defaults: self.group_parameters[group]
                        .iter()
                        .map(|parameter| self.type_parameter_defaults[parameter.index()])
                        .collect(),
                    parameters,
                    parameter_names,
                    object,
                    unavailable: false,
                },
            ));
        } else {
            self.interner.fill_object(reserved, object);
        }
        self.structural_finalizing.remove(&group);
        self.structural_finalized.insert(group);
        self.states[group] = GroupState::Ready;
    }

    fn materialize_object_type(&mut self, ty: TypeId) -> Option<ObjectType> {
        if let Some(object) = self.object_type_or_staged(ty) {
            return Some(object);
        }
        let native = if let Some(array) = self.interner.store().array_type(ty).copied() {
            Some((self.native_array_groups[0]?, array.element))
        } else if let Some(operand) = self.interner.store().readonly_operand(ty) {
            self.interner
                .store()
                .array_type(operand)
                .copied()
                .and_then(|array| self.native_array_groups[1].map(|group| (group, array.element)))
        } else {
            None
        };
        if let Some((group, element)) = native {
            self.finish_structural_group(group);
            let object = self.staged_objects.get(&group)?.clone();
            let base = self.interner.intern_object(object);
            let parameter = *self.group_parameters[group].first()?;
            let substitutions =
                FxHashMap::from_iter([(self.type_parameter_ids[parameter.index()], element)]);
            let materialized = substitute(self.interner, base, &substitutions);
            return self.object_type_or_staged(materialized);
        }
        let application = self.interner.store().instantiation_type(ty)?.clone();
        let base = if let Some(object) = self.object_type_or_staged(application.base) {
            self.interner.intern_object(object)
        } else {
            application.base
        };
        let substitutions = application.args.into_iter().collect::<FxHashMap<_, _>>();
        let materialized = substitute(self.interner, base, &substitutions);
        self.object_type_or_staged(materialized)
    }

    fn finish_type_parameter_defaults(&mut self) {
        let admitted = self.admitted_parameters.iter().copied().collect::<Vec<_>>();
        for index in admitted {
            let Some(recipe) = self.product.type_parameters()[index].default else {
                continue;
            };
            let owner = self.type_parameter_owner(index);
            self.type_parameter_defaults[index] = match self.lower_type(recipe, owner) {
                Ok(default) if default != self.interner.well_known().error => {
                    super::project_local_pass_port::PublishedTypeParameterDefault::Ready(default)
                }
                Ok(_) | Err(_) => {
                    self.fail_group_parameter(index);
                    super::project_local_pass_port::PublishedTypeParameterDefault::Unsupported
                }
            };
        }
    }

    fn finish_type_parameter_constraints(&mut self) {
        let admitted = self.admitted_parameters.iter().copied().collect::<Vec<_>>();
        for index in admitted {
            let Some(recipe) = self.product.type_parameters()[index].constraint else {
                continue;
            };
            let owner = self.type_parameter_owner(index);
            let Ok(constraint) = self.lower_type(recipe, owner) else {
                self.fail_group_parameter(index);
                continue;
            };
            if constraint == self.interner.well_known().error {
                self.fail_group_parameter(index);
                continue;
            }
            let parameter = self.type_parameter_ids[index];
            let installed = self.interner.store().type_param_constraint(parameter);
            if installed == Some(constraint) {
                continue;
            }
            if installed.is_some()
                || !self
                    .interner
                    .set_type_param_constraint(parameter, constraint)
            {
                self.fail_group_parameter(index);
            }
        }
    }

    fn fail_group_parameter(&mut self, parameter_index: usize) {
        let groups = self.parameter_groups[parameter_index]
            .iter()
            .copied()
            .collect::<Vec<_>>();
        for group in groups {
            self.fail_group_component(group);
        }
    }

    fn fail_group_component(&mut self, failed_group: usize) {
        let members = if self.group_scc_members[failed_group].is_empty() {
            vec![failed_group]
        } else {
            self.group_scc_members[failed_group].clone()
        };
        for group in members {
            if !self.admitted_groups.contains(&group) {
                continue;
            }
            if let Some(reserved) = self.group_value_types[group].take() {
                let _ = self.interner.abandon_reserved_object(reserved);
            }
            if let Some(reserved) = self.group_types[group] {
                if self.owned_object_reservations.remove(&reserved) {
                    let _ = self.interner.abandon_reserved_object(reserved);
                }
            }
            self.structural_drafts.remove(&group);
            self.staged_objects.remove(&group);
            self.structural_finalizing.remove(&group);
            self.structural_finalized.remove(&group);
            if let Some(local_group) = self.augmented_groups.get(&group) {
                self.overlays
                    .retain(|(candidate, _)| candidate != local_group);
            }
            self.group_types[group] = None;
            self.group_value_types[group] = None;
            self.states[group] = GroupState::Unavailable;
            self.value_states[group] = GroupState::Unavailable;
            self.resolving_alias_depth[group] = None;
            self.alias_bodies[group] = None;
            if self.alias_ids[group].is_some()
                && !self.recursive_alias_unavailable_groups.contains(&group)
                && self.alias_unavailable[group] != Some(PublishedAliasUnavailable::DirectCycle)
            {
                self.alias_unavailable[group] = Some(PublishedAliasUnavailable::ComponentFailure);
            }
        }
    }

    fn lower_alias(
        &mut self,
        group_index: usize,
        recipe: TypeRecipeKey,
        owner: IssueOwner,
    ) -> Result<TypeId, ProductLoweringError> {
        let template = self.alias_templates[group_index]
            .ok_or(ProductLoweringError::RecursiveAliasWithoutReservation)?;
        let body = self.lower_type(recipe, owner)?;
        self.alias_bodies[group_index] = Some(body);
        Ok(template)
    }

    fn lower_type(
        &mut self,
        recipe: TypeRecipeKey,
        owner: IssueOwner,
    ) -> Result<TypeId, ProductLoweringError> {
        let node = self.product.types()[recipe.index()].node.clone();
        match node {
            TypeRecipeNode::Keyword(KeywordKind::Intrinsic) => self.unsupported(
                recipe,
                owner,
                "annotation-lower/intrinsic-keyword/self",
                "intrinsic keyword type not modeled",
            ),
            TypeRecipeNode::Keyword(KeywordKind::This) => self.deferred_member_unsupported(
                recipe,
                owner,
                "annotation-lower/this-type/self",
                "this type annotation not modeled",
            ),
            TypeRecipeNode::Keyword(keyword) => self.lower_keyword(keyword),
            TypeRecipeNode::Literal(literal) => self.lower_literal(recipe, owner, literal),
            TypeRecipeNode::Reference {
                resolution,
                arguments,
                ..
            } => self.lower_reference(recipe, owner, resolution, &arguments),
            TypeRecipeNode::Query { resolution, .. } => {
                self.stage_qualified_path_validation(recipe, owner);
                if matches!(resolution, ValueReferenceResolution::IntrinsicGlobalValue) {
                    return self.unsupported(
                        recipe,
                        owner,
                        "annotation-lower/type-query/typeof",
                        "typeof type query not lowered",
                    );
                }
                self.unsupported(
                    recipe,
                    owner,
                    "annotation-lower/type-query/typeof",
                    "typeof type query not lowered",
                )
            }
            TypeRecipeNode::Import { arguments, .. } => {
                let _ = self.lower_types(&arguments, owner);
                self.unsupported(
                    recipe,
                    owner,
                    "annotation-lower/import-type/self",
                    "import type not modeled",
                )
            }
            TypeRecipeNode::Predicate { value, .. } => {
                if let Some(value) = value {
                    let _ = self.lower_type(value, owner);
                }
                self.deferred_member_unsupported(
                    recipe,
                    owner,
                    "annotation-lower/type-predicate/self",
                    "type predicate not lowered",
                )
            }
            TypeRecipeNode::Array(element) => {
                let element =
                    self.with_indirection(|lowerer| lowerer.lower_type(element, owner))?;
                Ok(self.interner.intern_array(element))
            }
            TypeRecipeNode::Tuple { elements } => {
                let mut fixed = Vec::new();
                let mut rest = None;
                let mut error = None;
                let mut recorded_optional = false;
                for (ordinal, element) in elements.iter().copied().enumerate() {
                    if element.optional {
                        if !recorded_optional {
                            self.stage_tuple_element_incomplete(
                                recipe,
                                ordinal,
                                owner,
                                element.site,
                                "annotation-lower/tuple-optional-element/self",
                                "optional tuple element aborts tuple lowering",
                            );
                            recorded_optional = true;
                        }
                        if error.is_none() {
                            error = Some(ProductLoweringError::UnsupportedRecipe);
                        }
                    }
                    let lowered = remember_lowering_error(
                        &mut error,
                        self.with_indirection(|lowerer| lowerer.lower_type(element.value, owner)),
                    );
                    if element.optional {
                        continue;
                    }
                    if element.rest {
                        let direct_infer = match self.product.types()[element.value.index()].node {
                            TypeRecipeNode::Infer(parameter) => Some(parameter),
                            _ => None,
                        };
                        if direct_infer.is_some_and(|parameter| {
                            self.product.type_parameters()[parameter.index()]
                                .constraint
                                .is_some()
                        }) {
                            if error.is_none() {
                                error = Some(ProductLoweringError::UnsupportedRecipe);
                            }
                            continue;
                        }
                        let array_like = match lowered {
                            Some(_) if direct_infer.is_some() => true,
                            Some(ty) => {
                                let mut seen = BTreeSet::new();
                                match self.tuple_rest_recipe_is_array_like(
                                    element.value,
                                    ty,
                                    owner,
                                    &mut seen,
                                ) {
                                    Ok(array_like) => array_like,
                                    Err(lowering_error) => {
                                        if error.is_none() {
                                            error = Some(lowering_error);
                                        }
                                        false
                                    }
                                }
                            }
                            None => false,
                        };
                        if array_like && rest.is_none() {
                            rest = lowered.map(|ty| TupleRestType::new(fixed.len(), ty));
                            continue;
                        }
                        if !array_like {
                            self.stage_tuple_element_incomplete(
                                recipe,
                                ordinal,
                                owner,
                                element.site,
                                "annotation-lower/tuple-rest-element/non-array",
                                "tuple rest element is not provably array-like",
                            );
                        }
                        if error.is_none() {
                            error = Some(ProductLoweringError::UnsupportedRecipe);
                        }
                    } else if let Some(ty) = lowered {
                        fixed.push(ty);
                    }
                }
                if let Some(error) = error {
                    return Err(error);
                }
                let tuple = match rest {
                    Some(rest) => TupleType::with_rest(fixed, rest),
                    None => TupleType::fixed(fixed),
                };
                Ok(self.interner.intern_tuple_type(tuple))
            }
            TypeRecipeNode::Union(types) => {
                let types = self.lower_types(&types, owner)?;
                Ok(self.interner.union(types))
            }
            TypeRecipeNode::Intersection(types) => {
                let types = self.lower_types(&types, owner)?;
                Ok(self.interner.intersection(types))
            }
            TypeRecipeNode::Function(signature) => {
                self.with_indirection(|lowerer| lowerer.lower_signature(signature, owner))
            }
            TypeRecipeNode::Constructor {
                signature,
                r#abstract: false,
            } => {
                let signature =
                    self.with_indirection(|lowerer| lowerer.lower_signature(signature, owner))?;
                Ok(self.interner.intern_object(ObjectType {
                    construct_signatures: Vec::from([signature]),
                    ..ObjectType::default()
                }))
            }
            TypeRecipeNode::Constructor {
                signature,
                r#abstract: true,
            } => {
                let _ = self.with_indirection(|lowerer| lowerer.lower_signature(signature, owner));
                self.unsupported(
                    recipe,
                    owner,
                    "annotation-lower/constructor-type/abstract",
                    "abstract constructor type not modeled",
                )
            }
            TypeRecipeNode::Object(members) => self.with_indirection(|lowerer| {
                let mut object = ObjectType::default();
                let mut omitted_overload_keys = Vec::new();
                lowerer.lower_members(
                    None,
                    &members,
                    &mut object,
                    false,
                    &mut omitted_overload_keys,
                )?;
                let error = lowerer.interner.well_known().error;
                object.properties.retain(|property| {
                    property.ty != error && !omitted_overload_keys.contains(&property.key)
                });
                Ok(lowerer.interner.intern_object(object))
            }),
            TypeRecipeNode::Operator { operator, operand } => {
                let operand = self.lower_type(operand, owner);
                match operator {
                    TypeOperator::Keyof => {
                        let operand = operand?;
                        let pending_object =
                            self.group_types
                                .iter()
                                .enumerate()
                                .any(|(group, reserved)| {
                                    *reserved == Some(operand)
                                        && !self.structural_finalized.contains(&group)
                                });
                        if pending_object {
                            return Ok(self.interner.intern_keyof(operand));
                        }
                        if let Some(keys) = keyof_of_type(self.interner, operand) {
                            return Ok(keys);
                        }
                        let tag = self.interner.store().tag(operand);
                        if matches!(
                            tag,
                            TypeTag::ClassInstance
                                | TypeTag::TypeParam
                                | TypeTag::Conditional
                                | TypeTag::Instantiation
                                | TypeTag::Mapped
                                | TypeTag::Template
                                | TypeTag::Keyof
                                | TypeTag::Infer
                                | TypeTag::MappedValue
                        ) {
                            Ok(self.interner.intern_keyof(operand))
                        } else {
                            self.unsupported(
                                recipe,
                                owner,
                                "annotation-lower/type-operator/keyof-operand",
                                "keyof operand has no modeled property keys",
                            )
                        }
                    }
                    TypeOperator::Readonly => Ok(self.interner.intern_readonly(operand?)),
                    TypeOperator::Unique => {
                        let _ = operand;
                        self.unsupported(
                            recipe,
                            owner,
                            "annotation-lower/type-operator/unique-operand",
                            "unique symbol operator not lowered",
                        )
                    }
                }
            }
            TypeRecipeNode::IndexedAccess { object, index } => {
                let mut error = None;
                let object = remember_lowering_error(&mut error, self.lower_type(object, owner));
                let index = remember_lowering_error(&mut error, self.lower_type(index, owner));
                if let Some(error) = error {
                    return Err(error);
                }
                Ok(self.interner.intern_deferred_indexed_access(
                    object.expect("successful indexed object"),
                    index.expect("successful indexed key"),
                ))
            }
            TypeRecipeNode::Mapped {
                parameter,
                name,
                value,
                optional,
                readonly,
            } => {
                let mapped =
                    self.lower_mapped(recipe, owner, parameter, name, value, (optional, readonly))?;
                Ok(self.interner.intern_mapped(mapped))
            }
            TypeRecipeNode::Conditional {
                check,
                extends,
                when_true,
                when_false,
            } => {
                let mut error = None;
                let check_recipe = check;
                let check =
                    remember_lowering_error(&mut error, self.lower_type(check_recipe, owner));
                let extends_ty =
                    remember_lowering_error(&mut error, self.lower_type(extends, owner));
                let true_branch = remember_lowering_error(
                    &mut error,
                    self.with_indirection(|lowerer| lowerer.lower_type(when_true, owner)),
                );
                let false_branch = remember_lowering_error(
                    &mut error,
                    self.with_indirection(|lowerer| lowerer.lower_type(when_false, owner)),
                );
                if let Some(error) = error {
                    return Err(error);
                }
                let conditional = ConditionalType {
                    check: check.expect("successful conditional check"),
                    extends_ty: extends_ty.expect("successful conditional constraint"),
                    true_branch: true_branch.expect("successful conditional true branch"),
                    false_branch: false_branch.expect("successful conditional false branch"),
                    infer_count: infer_count(self.product, extends),
                    distributive: is_type_parameter_reference(self.product, check_recipe),
                    poisoned: false,
                };
                Ok(self.interner.intern_conditional(conditional))
            }
            TypeRecipeNode::Infer(parameter) => {
                if let Some(constraint) =
                    self.product.type_parameters()[parameter.index()].constraint
                {
                    let _ = self.lower_type(constraint, owner);
                    return self.unsupported(
                        recipe,
                        owner,
                        "annotation-lower/infer-type/constraint",
                        "infer type constraint not lowered",
                    );
                }
                let index = u32::try_from(parameter.index())
                    .map_err(|_| ProductLoweringError::UnsupportedRecipe)?;
                Ok(self.interner.intern_infer(index))
            }
            TypeRecipeNode::Template { texts, holes } => {
                let texts = texts
                    .iter()
                    .map(|key| self.product.strings()[key.index()].clone())
                    .collect();
                let holes = self.lower_types(&holes, owner)?;
                Ok(self.interner.intern_template(TemplateType { texts, holes }))
            }
        }
    }

    fn lower_keyword(&self, keyword: KeywordKind) -> Result<TypeId, ProductLoweringError> {
        let well_known = self.interner.well_known();
        match keyword {
            KeywordKind::Any => Ok(well_known.any),
            KeywordKind::Unknown => Ok(well_known.unknown),
            KeywordKind::Never => Ok(well_known.never),
            KeywordKind::Void => Ok(well_known.void),
            KeywordKind::Null => Ok(well_known.null),
            KeywordKind::Undefined => Ok(well_known.undefined),
            KeywordKind::Boolean => Ok(well_known.boolean),
            KeywordKind::Number => Ok(well_known.number),
            KeywordKind::BigInt => Ok(well_known.bigint),
            KeywordKind::String => Ok(well_known.string),
            KeywordKind::Symbol => Ok(well_known.symbol),
            KeywordKind::Object => Ok(well_known.object),
            KeywordKind::Intrinsic | KeywordKind::This => unreachable!(
                "contextual keyword lowering records its typed incomplete before degradation"
            ),
        }
    }

    fn lower_literal(
        &mut self,
        recipe: TypeRecipeKey,
        owner: IssueOwner,
        literal: LiteralRecipe,
    ) -> Result<TypeId, ProductLoweringError> {
        let literal = match literal {
            LiteralRecipe::String(key) => super::project_local_pass_port::LiteralValue::String(
                self.product.strings()[key.index()].clone(),
            ),
            LiteralRecipe::NumberBits(bits) => {
                super::project_local_pass_port::LiteralValue::Number(f64::from_bits(bits))
            }
            LiteralRecipe::Boolean(value) => {
                super::project_local_pass_port::LiteralValue::Boolean(value)
            }
            LiteralRecipe::BigInt(_) => {
                return self.unsupported(
                    recipe,
                    owner,
                    "annotation-lower/bigint-literal/self",
                    "bigint literal type not modeled",
                );
            }
        };
        Ok(self.interner.intern_literal(literal))
    }

    fn lower_reference(
        &mut self,
        recipe: TypeRecipeKey,
        owner: IssueOwner,
        resolution: TypeReferenceResolution,
        arguments: &[TypeRecipeKey],
    ) -> Result<TypeId, ProductLoweringError> {
        if let Some(parameter) = self.contextual_type_parameters[recipe.index()] {
            return Ok(self.type_parameter_types[parameter.index()]);
        }
        match resolution {
            TypeReferenceResolution::Group(group) => {
                let group_index = group.index();
                if arguments.len() < self.group_parameters[group_index].len() {
                    self.default_applications.insert(
                        (recipe.index(), owner.identity),
                        PendingDefaultApplication {
                            group: group_index,
                            supplied: arguments.len(),
                            owner,
                        },
                    );
                }
                let was_resolving = self.states[group_index] == GroupState::Resolving;
                let lowered = self.lower_group(group_index);
                let structural =
                    self.product.groups()[group_index]
                        .fragments
                        .iter()
                        .any(|declaration| {
                            matches!(
                                self.product.declarations()[declaration.index()].payload,
                                DeclarationPayload::Class { .. }
                                    | DeclarationPayload::Interface { .. }
                            )
                        });
                let arguments_result = self.lower_types(arguments, owner);
                let Some(base) = lowered? else {
                    return self.unsupported(
                        recipe,
                        owner,
                        "annotation-lower/type-reference/missing-type",
                        "referenced declaration has no type surface",
                    );
                };
                let arguments = arguments_result?;
                let group_parameters = self.group_parameters[group_index].clone();
                let mut mapper = group_parameters
                    .iter()
                    .zip(arguments.iter().copied())
                    .map(|(parameter, argument)| {
                        (self.type_parameter_ids[parameter.index()], argument)
                    })
                    .collect::<Vec<_>>();
                for parameter in group_parameters.iter().skip(arguments.len()) {
                    let Some(default) = self.product.type_parameters()[parameter.index()].default
                    else {
                        break;
                    };
                    let default_owner = self.type_parameter_owner(parameter.index());
                    let default = self.lower_type(default, default_owner)?;
                    let substitutions = mapper.iter().copied().collect();
                    let default = substitute(self.interner, default, &substitutions);
                    mapper.push((self.type_parameter_ids[parameter.index()], default));
                }
                self.stage_constraint_validations(
                    recipe,
                    owner,
                    group_index,
                    &group_parameters,
                    &arguments,
                    &mapper,
                )?;
                if mapper.is_empty() {
                    return if self.interner.store().tag(base) == TypeTag::AliasTemplate {
                        Ok(self.interner.intern_instantiation(base, Vec::new()))
                    } else {
                        Ok(base)
                    };
                }
                if self.native_array_groups[0] == Some(group_index) {
                    return Ok(self.interner.intern_array(mapper[0].1));
                }
                if self.native_array_groups[1] == Some(group_index) {
                    let array = self.interner.intern_array(mapper[0].1);
                    return Ok(self.interner.intern_readonly(array));
                }
                let augmented = self.augmented_groups.contains_key(&group_index);
                if structural && !was_resolving && !augmented {
                    self.finish_structural_group(group_index);
                }
                let structurally_ready =
                    !structural || (!augmented && self.structural_finalized.contains(&group_index));
                let tag = self.interner.store().tag(base);
                let well_known = self.interner.well_known();
                let stays_lazy = was_resolving
                    || !structurally_ready
                    || matches!(
                        tag,
                        TypeTag::AliasTemplate | TypeTag::Conditional | TypeTag::Mapped
                    )
                    || well_known.is_string_intrinsic_marker(base)
                    || base == well_known.this_type
                    || base == well_known.omit_this_parameter;
                if stays_lazy {
                    Ok(self.interner.intern_instantiation(base, mapper))
                } else {
                    let substitutions = mapper.into_iter().collect();
                    Ok(substitute(self.interner, base, &substitutions))
                }
            }
            TypeReferenceResolution::TypeParameter(parameter) => {
                Ok(self.type_parameter_types[parameter.index()])
            }
            TypeReferenceResolution::Unresolved { .. } => {
                self.stage_qualified_path_validation(recipe, owner);
                self.unsupported(
                    recipe,
                    owner,
                    "annotation-lower/type-reference/unresolved",
                    "type reference could not be resolved",
                )
            }
        }
    }

    fn lower_types(
        &mut self,
        recipes: &[TypeRecipeKey],
        owner: IssueOwner,
    ) -> Result<Vec<TypeId>, ProductLoweringError> {
        let mut lowered = Vec::with_capacity(recipes.len());
        let mut error = None;
        for recipe in recipes {
            if let Some(ty) = remember_lowering_error(&mut error, self.lower_type(*recipe, owner)) {
                lowered.push(ty);
            }
        }
        error.map_or(Ok(lowered), Err)
    }

    fn stage_constraint_validations(
        &mut self,
        recipe: TypeRecipeKey,
        owner: IssueOwner,
        target_group: usize,
        parameters: &[TypeParameterKey],
        arguments: &[TypeId],
        mapper: &[(TypeParamId, TypeId)],
    ) -> Result<(), ProductLoweringError> {
        let Some(group) = self.current_validation_group else {
            return Ok(());
        };
        let application = mapper
            .iter()
            .map(|(_, argument)| *argument)
            .collect::<Vec<_>>();
        let application_arguments = match &self.product.types()[recipe.index()].node {
            TypeRecipeNode::Reference { arguments, .. } => arguments.to_vec(),
            _ => return Ok(()),
        };
        for (position, ((parameter, argument), argument_recipe)) in parameters
            .iter()
            .zip(arguments)
            .zip(&application_arguments)
            .enumerate()
        {
            let Some(constraint_recipe) =
                self.product.type_parameters()[parameter.index()].constraint
            else {
                continue;
            };
            let constraint_owner = self.type_parameter_owner(parameter.index());
            let _ = self.lower_type(constraint_recipe, constraint_owner)?;
            let argument_site = self.product.types()[argument_recipe.index()].site;
            self.validations.stage(ProjectLocalValidationEvent {
                producer: ProjectLocalValidationProducer::Constraint {
                    group,
                    recipe: recipe.index(),
                    parameter: parameter.index(),
                },
                candidate: ProjectLocalValidationCandidate::Constraint(
                    ProjectLocalConstraintValidation {
                        group,
                        target_group,
                        parameter: position,
                        site: self.validation_site(argument_site, owner.start),
                        argument: *argument,
                        application: application.clone(),
                    },
                ),
            });
        }
        Ok(())
    }

    fn tuple_rest_container_is_array_like(&self, ty: TypeId) -> bool {
        let store = self.interner.store();
        let mut current = ty;
        let mut seen = Vec::new();
        loop {
            match store.tag(current) {
                TypeTag::Array | TypeTag::Tuple => return true,
                TypeTag::Readonly => {
                    let Some(operand) = store.readonly_operand(current) else {
                        return false;
                    };
                    current = operand;
                }
                TypeTag::TypeParam => {
                    let Some(parameter) = store.type_param(current) else {
                        return false;
                    };
                    if seen.contains(&parameter.id) {
                        return false;
                    }
                    seen.push(parameter.id);
                    let Some(constraint) = store.type_param_constraint(parameter.id) else {
                        return false;
                    };
                    current = constraint;
                }
                _ => return false,
            }
        }
    }

    fn tuple_rest_recipe_is_array_like(
        &mut self,
        recipe: TypeRecipeKey,
        ty: TypeId,
        caller_owner: IssueOwner,
        seen: &mut BTreeSet<usize>,
    ) -> Result<bool, ProductLoweringError> {
        if self.tuple_rest_container_is_array_like(ty) {
            return Ok(true);
        }
        let TypeRecipeNode::Reference {
            resolution: TypeReferenceResolution::TypeParameter(parameter),
            ..
        } = self.product.types()[recipe.index()].node.clone()
        else {
            return Ok(false);
        };
        if !seen.insert(parameter.index()) {
            return Ok(false);
        }
        let Some(constraint) = self.product.type_parameters()[parameter.index()].constraint else {
            return Ok(false);
        };
        let owner = self.type_parameter_owners[parameter.index()].unwrap_or(caller_owner);
        let constraint_ty = self.lower_type(constraint, owner)?;
        self.tuple_rest_recipe_is_array_like(constraint, constraint_ty, owner, seen)
    }

    fn with_indirection<T>(&mut self, lower: impl FnOnce(&mut Self) -> T) -> T {
        self.alias_indirection_depth = self.alias_indirection_depth.saturating_add(1);
        let result = lower(self);
        self.alias_indirection_depth = self.alias_indirection_depth.saturating_sub(1);
        result
    }

    fn lower_signature(
        &mut self,
        key: SignatureKey,
        owner: IssueOwner,
    ) -> Result<TypeId, ProductLoweringError> {
        let signature = self.product.signatures()[key.index()].clone();
        let mut error = None;
        let mut type_params = Vec::with_capacity(signature.type_parameters.len());
        for parameter in signature.type_parameters.iter() {
            let row = self.product.type_parameters()[parameter.index()].clone();
            let constraint = row.constraint.and_then(|recipe| {
                remember_lowering_error(&mut error, self.lower_type(recipe, owner))
            });
            let default = row.default.and_then(|recipe| {
                remember_lowering_error(&mut error, self.lower_type(recipe, owner))
            });
            type_params.push(GenericTypeParam {
                id: self.type_parameter_ids[parameter.index()],
                constraint,
                default,
            });
        }
        let mut receiver = None;
        let mut params = Vec::new();
        for (index, parameter) in IntoIterator::into_iter(signature.parameters).enumerate() {
            let ty = match parameter.value {
                Some(recipe) => remember_lowering_error(&mut error, self.lower_type(recipe, owner)),
                None => Some(self.interner.well_known().any),
            };
            let Some(ty) = ty else {
                continue;
            };
            if parameter.this_parameter {
                receiver = Some(ty);
                continue;
            }
            let name = match parameter.pattern {
                ParameterPattern::Identifier(name) => self.product.strings()[name.index()].clone(),
                ParameterPattern::Array(_) | ParameterPattern::Object(_) => {
                    format!("_destructured{index}")
                }
            };
            let parameter = if parameter.rest {
                ParameterType::rest(name, ty)
            } else if parameter.optional {
                ParameterType::optional(name, ty)
            } else {
                ParameterType::required(name, ty)
            };
            params.push(parameter);
        }
        let ret = match signature.result {
            Some(recipe) => remember_lowering_error(&mut error, self.lower_type(recipe, owner)),
            None => Some(self.interner.well_known().void),
        };
        if let Some(error) = error {
            return Err(error);
        }
        let ret = ret.expect("a successful signature has a return type");
        Ok(self.interner.intern_function(FunctionType {
            type_params,
            receiver,
            params,
            ret,
        }))
    }

    fn lower_members(
        &mut self,
        group_index: Option<usize>,
        members: &[MemberKey],
        object: &mut ObjectType,
        class_members: bool,
        omitted_overload_keys: &mut Vec<PropertyKey>,
    ) -> Result<(), ProductLoweringError> {
        let may_defer_member = !class_members
            && group_index.is_none_or(|group_index| {
                self.product.groups()[group_index]
                    .fragments
                    .iter()
                    .all(|declaration| {
                        !matches!(
                            self.product.declarations()[declaration.index()].payload,
                            DeclarationPayload::Class { .. }
                        )
                    })
            });
        let mut error = None;
        for key in members {
            let member = self.product.members()[key.index()].clone();
            if class_members && (member.static_member || member.kind == MemberKind::Constructor) {
                continue;
            }
            let overload_key = (member.kind == MemberKind::Method)
                .then(|| {
                    member
                        .name
                        .as_ref()
                        .and_then(|name| property_key(self.product, name))
                })
                .flatten();
            let result = if has_unsupported_computed_name(self.product, &member) {
                self.lower_deferred_computed_member(*key, &member)
            } else {
                self.lower_member(*key, &member, object, class_members)
            };
            match result {
                Ok(()) => {}
                Err(
                    ProductLoweringError::DeferredMember | ProductLoweringError::UnsupportedRecipe,
                ) if may_defer_member => {
                    if let Some(key) = overload_key.clone() {
                        if !omitted_overload_keys.contains(&key) {
                            omitted_overload_keys.push(key);
                        }
                    }
                }
                Err(ProductLoweringError::DeferredMember) => {
                    if error.is_none() {
                        error = Some(ProductLoweringError::UnsupportedRecipe);
                    }
                }
                Err(member_error) => {
                    if error.is_none() {
                        error = Some(member_error);
                    }
                }
            }
            if let Some(key) = overload_key {
                if omitted_overload_keys.contains(&key) {
                    object.properties.retain(|property| property.key != key);
                }
            }
        }
        error.map_or(Ok(()), Err)
    }

    fn lower_member(
        &mut self,
        key: MemberKey,
        member: &super::project_local_pass_port::MemberRecipe,
        target: &mut ObjectType,
        class_members: bool,
    ) -> Result<(), ProductLoweringError> {
        self.lower_member_inner(key, member, target, class_members)
    }

    fn lower_member_inner(
        &mut self,
        key: MemberKey,
        member: &super::project_local_pass_port::MemberRecipe,
        target: &mut ObjectType,
        class_members: bool,
    ) -> Result<(), ProductLoweringError> {
        let owner = self.member_owner(key);
        let mut draft = target.clone();
        let object = &mut draft;
        match member.kind {
            MemberKind::Call => {
                let signature = self.required_member_signature(key, member.signature)?;
                object.call_signatures.push(
                    self.with_indirection(|lowerer| lowerer.lower_signature(signature, owner))?,
                );
            }
            MemberKind::Construct | MemberKind::Constructor => {
                let signature = self.required_member_signature(key, member.signature)?;
                object.construct_signatures.push(
                    self.with_indirection(|lowerer| lowerer.lower_signature(signature, owner))?,
                );
            }
            MemberKind::Index => {
                let signature = self.required_member_signature(key, member.signature)?;
                let row = self.product.signatures()[signature.index()].clone();
                let mut error = None;
                let key_type = match row.parameters.first().and_then(|parameter| parameter.value) {
                    Some(parameter) => {
                        remember_lowering_error(&mut error, self.lower_type(parameter, owner))
                    }
                    None => remember_lowering_error(
                        &mut error,
                        self.unsupported_member(
                            key,
                            "annotation-lower/index-signature/missing-key",
                            "index signature key type missing",
                        ),
                    ),
                };
                let value = match row.result {
                    Some(recipe) => remember_lowering_error(
                        &mut error,
                        self.with_indirection(|lowerer| lowerer.lower_type(recipe, owner)),
                    ),
                    None => Some(self.interner.well_known().any),
                };
                if let Some(error) = error {
                    return Err(error);
                }
                let key_type = key_type.expect("successful index key lowering");
                let value = value.expect("successful index value lowering");
                let well_known = self.interner.well_known();
                if key_type == well_known.string {
                    object.string_index = Some(value);
                } else if key_type == well_known.number {
                    object.number_index = Some(value);
                } else {
                    return self.unsupported_member(
                        key,
                        "annotation-lower/index-signature/key-type",
                        "index signature key type not modeled",
                    );
                }
            }
            MemberKind::Method => {
                let property_key = self.required_member_property_key(key, member.name.as_ref())?;
                let signature_key = self.required_member_signature(key, member.signature)?;
                let signature =
                    self.with_indirection(|lowerer| lowerer.lower_signature(signature_key, owner))?;
                if member.optional {
                    self.stage_member_incomplete(
                        key,
                        member.site,
                        "annotation-lower/optional-method/self",
                        "optional method signature not modeled",
                    );
                    return Err(ProductLoweringError::DeferredMember);
                }
                if let Some(existing) = object
                    .properties
                    .iter_mut()
                    .find(|property| property.key == property_key)
                {
                    if existing.ty != self.interner.well_known().error {
                        let mut overloads = self
                            .interner
                            .store()
                            .object_type(existing.ty)
                            .filter(|surface| {
                                surface.properties.is_empty()
                                    && surface.string_index.is_none()
                                    && surface.number_index.is_none()
                                    && surface.construct_signatures.is_empty()
                            })
                            .cloned()
                            .unwrap_or_else(|| ObjectType {
                                call_signatures: vec![existing.ty],
                                ..ObjectType::default()
                            });
                        overloads.call_signatures.push(signature);
                        existing.ty = self.interner.intern_object(overloads);
                    }
                } else {
                    object.properties.push(PropertyType {
                        key: property_key,
                        ty: signature,
                        write_ty: None,
                        optional: member.optional,
                        visibility: member_visibility(member.accessibility),
                        declaring_class: None,
                        readonly: member.readonly,
                        is_accessor: false,
                    });
                }
            }
            MemberKind::Getter | MemberKind::Setter => {
                let property_key = self.required_member_property_key(key, member.name.as_ref())?;
                if !class_members {
                    let signature = self.required_member_signature(key, member.signature)?;
                    let _ =
                        self.with_indirection(|lowerer| lowerer.lower_signature(signature, owner))?;
                    self.stage_member_incomplete(
                        key,
                        member.site,
                        "annotation-lower/interface-accessor/self",
                        "interface accessor signature not modeled",
                    );
                    return Err(ProductLoweringError::DeferredMember);
                }
                let signature_key = self.required_member_signature(key, member.signature)?;
                let signature_row = self.product.signatures()[signature_key.index()].clone();
                let signature =
                    self.with_indirection(|lowerer| lowerer.lower_signature(signature_key, owner))?;
                let Some(signature) = self.interner.store().function_type(signature).cloned()
                else {
                    return self.unsupported_member(
                        key,
                        "annotation-lower/accessor/signature-shape",
                        "accessor did not lower to a function signature",
                    );
                };
                let accessor_ty = if member.kind == MemberKind::Getter {
                    if signature_row.result.is_some() {
                        signature.ret
                    } else {
                        self.interner.well_known().any
                    }
                } else {
                    if signature_row
                        .parameters
                        .first()
                        .and_then(|parameter| parameter.value)
                        .is_some()
                    {
                        signature
                            .params
                            .first()
                            .map_or(self.interner.well_known().any, |parameter| parameter.ty)
                    } else {
                        self.interner.well_known().any
                    }
                };
                if let Some(existing) = object
                    .properties
                    .iter_mut()
                    .find(|property| property.key == property_key && property.is_accessor)
                {
                    if member.kind == MemberKind::Getter {
                        existing.ty = accessor_ty;
                    } else {
                        existing.write_ty = Some(accessor_ty);
                        existing.readonly = false;
                    }
                } else {
                    object.properties.push(PropertyType {
                        key: property_key,
                        ty: accessor_ty,
                        write_ty: (member.kind == MemberKind::Setter).then_some(accessor_ty),
                        optional: member.optional,
                        visibility: member_visibility(member.accessibility),
                        declaring_class: None,
                        readonly: member.kind == MemberKind::Getter,
                        is_accessor: true,
                    });
                }
            }
            MemberKind::Property => {
                let property_key = self.required_member_property_key(key, member.name.as_ref())?;
                let ty = member
                    .value
                    .map(|recipe| {
                        self.with_indirection(|lowerer| lowerer.lower_type(recipe, owner))
                    })
                    .transpose()?
                    .unwrap_or(self.interner.well_known().any);
                let ty = if member.optional {
                    self.interner
                        .union(vec![ty, self.interner.well_known().undefined])
                } else {
                    ty
                };
                let visibility = match member.accessibility {
                    Some(Accessibility::Protected) => Visibility::Protected,
                    Some(Accessibility::Private) => Visibility::Private,
                    Some(Accessibility::Public) | None => Visibility::Public,
                };
                object.properties.push(PropertyType {
                    key: property_key,
                    ty,
                    write_ty: None,
                    optional: member.optional,
                    visibility,
                    declaring_class: None,
                    readonly: member.readonly,
                    is_accessor: false,
                });
            }
        }
        *target = draft;
        Ok(())
    }

    fn lower_deferred_computed_member(
        &mut self,
        key: MemberKey,
        member: &super::project_local_pass_port::MemberRecipe,
    ) -> Result<(), ProductLoweringError> {
        let owner = self.member_owner(key);
        let result = match member.kind {
            MemberKind::Property => member
                .value
                .map(|recipe| self.lower_type(recipe, owner))
                .transpose()
                .map(|_| ()),
            MemberKind::Method | MemberKind::Getter | MemberKind::Setter => {
                let signature = self.required_member_signature(key, member.signature)?;
                self.with_indirection(|lowerer| lowerer.lower_signature(signature, owner))
                    .map(|_| ())
            }
            MemberKind::Call
            | MemberKind::Construct
            | MemberKind::Index
            | MemberKind::Constructor => Err(ProductLoweringError::DeferredMember),
        };
        let Some(PropertyNameRecipe::Computed { site, .. }) = member.name.as_ref() else {
            return result;
        };
        let (id, context) = if member.kind == MemberKind::Property {
            (
                "signature/property-signature/computed-key",
                "computed property signature key not visited",
            )
        } else {
            (
                "signature/method-signature/computed-key",
                "computed method signature key not visited",
            )
        };
        self.stage_member_incomplete(key, *site, id, context);
        match result {
            Err(ProductLoweringError::MissingAdmission(_))
            | Err(ProductLoweringError::RecursiveAliasWithoutReservation) => result,
            Err(ProductLoweringError::UnsupportedRecipe | ProductLoweringError::DeferredMember) => {
                Err(ProductLoweringError::DeferredMember)
            }
            Ok(()) => Err(ProductLoweringError::DeferredMember),
        }
    }

    fn required_member_signature(
        &mut self,
        member: MemberKey,
        signature: Option<SignatureKey>,
    ) -> Result<SignatureKey, ProductLoweringError> {
        let Some(signature) = signature else {
            return self.unsupported_member(
                member,
                "annotation-lower/member/missing-signature",
                "member signature missing",
            );
        };
        Ok(signature)
    }

    fn required_member_property_key(
        &mut self,
        member: MemberKey,
        name: Option<&PropertyNameRecipe>,
    ) -> Result<PropertyKey, ProductLoweringError> {
        let Some(name) = name else {
            return self.unsupported_member(
                member,
                "annotation-lower/member/missing-name",
                "named member has no property name",
            );
        };
        let Some(key) = property_key(self.product, name) else {
            return self.unsupported_member(
                member,
                "annotation-lower/member/property-key",
                "member property key not modeled",
            );
        };
        Ok(key)
    }

    fn lower_mapped(
        &mut self,
        recipe: TypeRecipeKey,
        owner: IssueOwner,
        parameter: TypeParameterKey,
        name: Option<TypeRecipeKey>,
        value: Option<TypeRecipeKey>,
        modifiers: (MappedModifier, MappedModifier),
    ) -> Result<MappedType, ProductLoweringError> {
        let (optional, readonly) = modifiers;
        let parameter_row = self.product.type_parameters()[parameter.index()].clone();
        let mut error = None;
        if let Some(name) = name {
            let _ = remember_lowering_error(&mut error, self.lower_type(name, owner));
            self.stage_recipe_incomplete(
                recipe,
                owner,
                "annotation-lower/mapped-type/name-remap",
                "mapped type name remapping not modeled",
            );
            if error.is_none() {
                error = Some(ProductLoweringError::UnsupportedRecipe);
            }
        }
        let key_source = parameter_row.constraint.and_then(|constraint| {
            match self.product.types()[constraint.index()].node {
                TypeRecipeNode::Operator {
                    operator: TypeOperator::Keyof,
                    operand,
                } => remember_lowering_error(&mut error, self.lower_type(operand, owner)),
                _ => remember_lowering_error(&mut error, self.lower_type(constraint, owner)),
            }
        });
        if parameter_row.constraint.is_none() {
            self.stage_recipe_incomplete(
                recipe,
                owner,
                "annotation-lower/mapped-type/missing-constraint",
                "mapped type parameter constraint missing",
            );
            if error.is_none() {
                error = Some(ProductLoweringError::UnsupportedRecipe);
            }
        }
        let value_template = match value {
            Some(value) => remember_lowering_error(&mut error, self.lower_type(value, owner)),
            None => Some(self.interner.well_known().any),
        };
        if let Some(error) = error {
            return Err(error);
        }
        Ok(MappedType {
            homomorphic: parameter_row.constraint.is_some_and(|constraint| {
                matches!(
                    self.product.types()[constraint.index()].node,
                    TypeRecipeNode::Operator {
                        operator: TypeOperator::Keyof,
                        ..
                    }
                )
            }),
            key_source: key_source.expect("successful mapped key source"),
            value_template: value_template.expect("successful mapped value"),
            modifiers_source: None,
            optional_modifier: mapped_modifier(optional),
            readonly_modifier: mapped_modifier(readonly),
        })
    }

    fn unsupported<T>(
        &mut self,
        recipe: TypeRecipeKey,
        owner: IssueOwner,
        id: &'static str,
        context: &'static str,
    ) -> Result<T, ProductLoweringError> {
        self.stage_recipe_incomplete(recipe, owner, id, context);
        Err(ProductLoweringError::UnsupportedRecipe)
    }

    fn deferred_member_unsupported(
        &mut self,
        recipe: TypeRecipeKey,
        owner: IssueOwner,
        id: &'static str,
        context: &'static str,
    ) -> Result<TypeId, ProductLoweringError> {
        self.stage_recipe_incomplete(recipe, owner, id, context);
        Err(ProductLoweringError::DeferredMember)
    }

    fn stage_recipe_incomplete(
        &mut self,
        recipe: TypeRecipeKey,
        owner: IssueOwner,
        id: &'static str,
        context: &'static str,
    ) {
        let site = self.product.types()[recipe.index()].site;
        self.stage_incomplete(
            LibraryRecordProducer {
                domain: "recipe",
                subject: recipe.index(),
                owner: owner.identity,
                ordinal: 0,
            },
            site,
            owner,
            id,
            context,
        );
    }

    fn stage_tuple_element_incomplete(
        &mut self,
        recipe: TypeRecipeKey,
        ordinal: usize,
        owner: IssueOwner,
        site: SourceSite,
        id: &'static str,
        context: &'static str,
    ) {
        self.stage_incomplete(
            LibraryRecordProducer {
                domain: "tuple-element",
                subject: recipe.index(),
                owner: owner.identity,
                ordinal,
            },
            site,
            owner,
            id,
            context,
        );
    }

    fn stage_member_incomplete(
        &mut self,
        member: MemberKey,
        site: SourceSite,
        id: &'static str,
        context: &'static str,
    ) {
        let owner = self.member_owner(member);
        self.stage_incomplete(
            LibraryRecordProducer {
                domain: "member",
                subject: member.index(),
                owner: owner.identity,
                ordinal: 0,
            },
            site,
            owner,
            id,
            context,
        );
    }

    fn unsupported_member<T>(
        &mut self,
        member: MemberKey,
        id: &'static str,
        context: &'static str,
    ) -> Result<T, ProductLoweringError> {
        let site = self.product.members()[member.index()].site;
        self.stage_member_incomplete(member, site, id, context);
        Err(ProductLoweringError::UnsupportedRecipe)
    }

    fn unsupported_group<T>(
        &mut self,
        group: usize,
        id: &'static str,
        context: &'static str,
    ) -> Result<T, ProductLoweringError> {
        let declaration = self.product.groups()[group]
            .fragments
            .first()
            .copied()
            .expect("a declaration group has a fragment");
        let row = &self.product.declarations()[declaration.index()];
        let owner = IssueOwner {
            identity: LibraryRecordOwnerIdentity::Declaration(declaration.index()),
            start: row.binding_site.start,
        };
        self.stage_incomplete(
            LibraryRecordProducer {
                domain: "declaration-group",
                subject: group,
                owner: owner.identity,
                ordinal: 0,
            },
            row.site,
            owner,
            id,
            context,
        );
        Err(ProductLoweringError::UnsupportedRecipe)
    }

    fn unsupported_declaration<T>(
        &mut self,
        declaration: DeclarationKey,
        id: &'static str,
        context: &'static str,
    ) -> Result<T, ProductLoweringError> {
        let row = &self.product.declarations()[declaration.index()];
        let site = row.site;
        let owner = IssueOwner {
            identity: LibraryRecordOwnerIdentity::Declaration(declaration.index()),
            start: row.binding_site.start,
        };
        self.stage_incomplete(
            LibraryRecordProducer {
                domain: "declaration",
                subject: declaration.index(),
                owner: owner.identity,
                ordinal: 0,
            },
            site,
            owner,
            id,
            context,
        );
        Err(ProductLoweringError::UnsupportedRecipe)
    }

    fn stage_incomplete(
        &mut self,
        producer: LibraryRecordProducer,
        site: SourceSite,
        owner: IssueOwner,
        id: &'static str,
        context: &'static str,
    ) {
        let source = &self.product.sources()[site.source.index()];
        self.records.stage(ProjectLocalLibraryRecordEvent {
            identity: LibraryRecordEventIdentity { producer, rule: id },
            normalized: ProjectLocalLibraryRecordForTest {
                source_path: self.product.strings()[source.name.index()].clone(),
                owner_start: owner.start,
                record: NormalizedCheckerRecordForTest::Incomplete(NormalizedIncompleteForTest {
                    span: NormalizedSpanForTest {
                        start: site.start,
                        end: site.end,
                    },
                    id: id.to_owned(),
                    context: context.to_owned(),
                }),
            },
        });
    }

    fn finish_default_application_records(&mut self) {
        let applications = std::mem::take(&mut self.default_applications);
        for ((recipe, _), application) in applications {
            let unsupported = self.group_parameters[application.group]
                .iter()
                .skip(application.supplied)
                .take_while(|parameter| {
                    !matches!(
                        self.type_parameter_defaults[parameter.index()],
                        super::project_local_pass_port::PublishedTypeParameterDefault::Absent
                    )
                })
                .any(|parameter| {
                    matches!(
                        self.type_parameter_defaults[parameter.index()],
                        super::project_local_pass_port::PublishedTypeParameterDefault::Unsupported
                    )
                });
            if !unsupported {
                continue;
            }
            let recipe = self
                .product
                .type_recipe_key_at(recipe)
                .expect("default application recipe exists");
            let site = self.product.types()[recipe.index()].site;
            self.stage_incomplete(
                LibraryRecordProducer {
                    domain: "default-application",
                    subject: recipe.index(),
                    owner: application.owner.identity,
                    ordinal: 0,
                },
                site,
                application.owner,
                "annotation-lower/type-reference/default-argument",
                "type-parameter default unavailable at application",
            );
        }
    }

    fn lower_intrinsic_marker(
        &mut self,
        declaration: DeclarationKey,
        marker: IntrinsicMarker,
    ) -> Result<TypeId, ProductLoweringError> {
        match marker {
            IntrinsicMarker::Uppercase => Ok(self.interner.well_known().uppercase),
            IntrinsicMarker::Lowercase => Ok(self.interner.well_known().lowercase),
            IntrinsicMarker::Capitalize => Ok(self.interner.well_known().capitalize),
            IntrinsicMarker::Uncapitalize => Ok(self.interner.well_known().uncapitalize),
            IntrinsicMarker::BuiltinIteratorReturn => Ok(self.interner.well_known().undefined),
            IntrinsicMarker::NoInfer => {
                let DeclarationPayload::TypeAlias {
                    type_parameters, ..
                } = self.product.declarations()[declaration.index()]
                    .payload
                    .clone()
                else {
                    return self.unsupported_declaration(
                        declaration,
                        "annotation-lower/intrinsic-marker/no-infer-shape",
                        "NoInfer intrinsic marker requires one type parameter",
                    );
                };
                if type_parameters.len() != 1 {
                    return self.unsupported_declaration(
                        declaration,
                        "annotation-lower/intrinsic-marker/no-infer-shape",
                        "NoInfer intrinsic marker requires one type parameter",
                    );
                }
                let parameter = type_parameters[0];
                Ok(self.type_parameter_types[parameter.index()])
            }
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) struct SemanticProbeInput<'a> {
    pub(super) materialized: &'a [usize],
    pub(super) group_types: &'a [Option<TypeId>],
    pub(super) group_value_types: &'a [Option<TypeId>],
    pub(super) final_type_group_fragment_counts: &'a BTreeMap<usize, usize>,
    pub(super) final_value_declaration_counts: &'a BTreeMap<usize, usize>,
    pub(super) authority: &'a super::project_local_pass_port::ProjectLocalProbeAuthority,
    pub(super) include_namespace_values: bool,
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn build_semantic_probes(
    interner: &mut Interner,
    mut query_state: SemanticQueryState,
    input: SemanticProbeInput<'_>,
) -> ProjectLocalSemanticProbesForTest {
    let SemanticProbeInput {
        materialized,
        group_types,
        group_value_types,
        final_type_group_fragment_counts,
        final_value_declaration_counts,
        authority,
        include_namespace_values,
    } = input;
    let published_classes = PublishedClasses::empty();
    let mut next_type_parameter =
        u32::try_from(group_types.len()).expect("project-local group count fits u32");
    let mut global_types = Vec::new();
    let mut module_types = Vec::new();
    let mut global_values = Vec::new();
    let materialized = materialized.iter().copied().collect::<BTreeSet<_>>();
    for coordinate in &authority.types {
        let admitted = coordinate
            .product_keys
            .iter()
            .copied()
            .filter(|key| materialized.contains(key))
            .collect::<Vec<_>>();
        if admitted.is_empty() {
            continue;
        }
        let Some(semantic) = admitted
            .iter()
            .find_map(|key| group_types.get(*key).copied().flatten())
        else {
            continue;
        };
        let mut member_keys = probe_object_type(interner, semantic)
            .map(|object| {
                object
                    .properties
                    .iter()
                    .map(|property| property.key.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        member_keys.sort();
        member_keys.dedup();
        let member_names = member_keys.into_iter().map(|key| key.to_string()).collect();
        let declaration_count = final_type_group_fragment_counts
            .get(&coordinate.identity)
            .copied()
            .unwrap_or(0);
        let probe = super::project_local_pass_port::NormalizedTypeProbeForTest {
            name: coordinate.name.clone(),
            declaration_count,
            member_names,
        };
        if let Some(source_path) = &coordinate.source_path {
            module_types.push(ProjectLocalModuleTypeProbeForTest {
                source_path: source_path.clone(),
                probe,
            });
        } else {
            global_types.push(probe);
        }
    }
    for coordinate in &authority.values {
        let namespace_product_keys = if include_namespace_values {
            coordinate.namespace_product_keys.as_slice()
        } else {
            &[]
        };
        let admitted = coordinate
            .product_keys
            .iter()
            .chain(namespace_product_keys)
            .copied()
            .filter(|key| materialized.contains(key))
            .collect::<Vec<_>>();
        if admitted.is_empty() {
            continue;
        }
        let Some(semantic) = admitted
            .iter()
            .find_map(|key| group_value_types.get(*key).copied().flatten())
        else {
            continue;
        };
        let mut member_names = Vec::new();
        let mut signatures = Vec::new();
        if let Some(function) = interner.store().function_type(semantic).cloned() {
            signatures.push(signature_probe(
                interner,
                &published_classes,
                &mut query_state,
                &mut next_type_parameter,
                function,
            ));
        }
        if let Some(object) = probe_object_type(interner, semantic).cloned() {
            member_names.extend(
                object
                    .properties
                    .iter()
                    .map(|property| property.key.to_string()),
            );
            signatures.extend(
                object
                    .call_signatures
                    .iter()
                    .filter_map(|signature| interner.store().function_type(*signature).cloned())
                    .collect::<Vec<_>>()
                    .into_iter()
                    .map(|signature| {
                        signature_probe(
                            interner,
                            &published_classes,
                            &mut query_state,
                            &mut next_type_parameter,
                            signature,
                        )
                    }),
            );
        }
        stable_dedup(&mut member_names);
        global_values.push(ProjectLocalValueProbeForTest {
            name: coordinate.name.clone(),
            declaration_count: final_value_declaration_counts
                .get(&coordinate.identity)
                .copied()
                .unwrap_or(0),
            call_signature_count: signatures.len(),
            signatures,
            member_names,
            callable_members: Vec::new(),
        });
    }
    global_types.sort_by(|left, right| left.name.cmp(&right.name));
    module_types.sort_by(|left, right| {
        (&left.source_path, &left.probe.name).cmp(&(&right.source_path, &right.probe.name))
    });
    global_values.sort_by(|left, right| left.name.cmp(&right.name));
    ProjectLocalSemanticProbesForTest {
        global_types,
        module_types,
        global_values,
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn stable_dedup(rows: &mut Vec<String>) {
    let mut seen = BTreeSet::new();
    rows.retain(|row| seen.insert(row.clone()));
}

#[cfg(any(test, feature = "test-utils"))]
fn probe_object_type(interner: &Interner, mut ty: TypeId) -> Option<&ObjectType> {
    loop {
        if let Some(object) = interner.store().object_type(ty) {
            return Some(object);
        }
        ty = interner.store().instantiation_type(ty)?.base;
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn signature_probe(
    interner: &mut Interner,
    published_classes: &PublishedClasses,
    query_state: &mut SemanticQueryState,
    next_type_parameter: &mut u32,
    signature: FunctionType,
) -> super::project_local_pass_port::SignatureProbe {
    let demand = |interner: &mut Interner,
                  query_state: &mut SemanticQueryState,
                  next_type_parameter: &mut u32,
                  ty| {
        let outcome = {
            let mut query = SemanticQueryCoordinator::new(
                interner,
                published_classes,
                query_state,
                next_type_parameter,
            );
            query.demand(ty)
        };
        match outcome {
            DemandOutcome::Ready(ready) => ready,
            DemandOutcome::Exhausted(_) => ty,
        }
    };
    super::project_local_pass_port::SignatureProbe {
        parameter_types: signature
            .params
            .iter()
            .map(|parameter| {
                let ty = demand(interner, query_state, next_type_parameter, parameter.ty);
                render_type(interner.store(), ty, false)
            })
            .collect(),
        return_type: {
            let ty = demand(interner, query_state, next_type_parameter, signature.ret);
            render_type(interner.store(), ty, false)
        },
    }
}

#[cfg(test)]
pub struct ProductLoweringForTest {
    pub group_types: Vec<Option<TypeId>>,
    pub ready: BTreeSet<usize>,
    pub unavailable: BTreeSet<usize>,
    pub missing_admissions: BTreeSet<usize>,
    pub aliases: PublishedAliases,
    pub library_records: Vec<ProjectLocalLibraryRecordForTest>,
    library_record_events: Vec<ProjectLocalLibraryRecordEvent>,
}

#[cfg(test)]
pub fn lower_product_groups_for_test(
    product: &LibraryDeclarationProduct,
    interner: &mut Interner,
    materialized: &[usize],
    augmented_groups: &BTreeMap<usize, TypeGroupId>,
    native_array_groups: [Option<usize>; 2],
    seed: Option<()>,
) -> ProductLoweringForTest {
    debug_assert!(seed.is_none());
    let receipt = lower_product_groups(
        product,
        interner,
        materialized,
        augmented_groups,
        native_array_groups,
        None,
    );
    let library_record_events = receipt.library_record_events;
    let mut records = LibraryRecordAccumulator::default();
    records.extend(library_record_events.clone());
    ProductLoweringForTest {
        group_types: receipt.group_types,
        ready: receipt.ready,
        unavailable: receipt.unavailable,
        missing_admissions: receipt.missing_admissions,
        aliases: receipt.aliases,
        library_records: records.normalized(),
        library_record_events,
    }
}

#[cfg(test)]
pub fn accumulate_product_library_records_for_test(
    receipts: &[&ProductLoweringForTest],
) -> Vec<ProjectLocalLibraryRecordForTest> {
    let mut records = LibraryRecordAccumulator::default();
    for receipt in receipts {
        records.extend(receipt.library_record_events.clone());
    }
    records.normalized()
}

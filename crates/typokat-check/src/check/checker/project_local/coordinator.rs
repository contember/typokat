//! Product-key scheduler and atomic terminal publication.

use super::lowerer::{lower_product_groups, ProductLoweringMetadataPlan, ProductLoweringSeed};
#[cfg(any(test, feature = "test-utils"))]
use super::observation::ProjectLocalSemanticPhaseForTest;
use super::project_local_pass_port::{
    project_local_published_types, ContainerKind, DeclarationPayload, DependencyKind,
    DependencyTarget, LibraryDeclarationProduct, ProjectLocalDemandSnapshot,
    ProjectLocalPublishedTypeInputs, ProjectLocalTypeOverlayDraft, ProjectLocalUserDemandGateway,
    PublishedAliases, TypeGroupId, TypeParamId, ValueStorageId,
};
#[cfg(any(test, feature = "test-utils"))]
use super::project_local_pass_port::{
    Interner, IntrinsicKind, LiteralValue, PublishedAliasTerminal, PublishedAliasUnavailable,
    TypeId, TypeTag,
};
use super::record_staging::LibraryRecordAccumulator;
use super::session::SessionCore;
use super::terminal::ProductTerminal;
use super::validation::{ProjectLocalValidationAccumulator, ProjectLocalValidationCandidate};
#[cfg(any(test, feature = "test-utils"))]
use super::{
    ProjectLocalCacheWriteForTest, ProjectLocalForceAllAliasTerminalForTest,
    ProjectLocalForceAllAliasUnavailableForTest, ProjectLocalForceAllIntrinsicKindForTest,
    ProjectLocalForceAllLiteralKindForTest, ProjectLocalForceAllObjectShapeForTest,
    ProjectLocalForceAllReadySurfaceForTest, ProjectLocalForceAllTerminalForTest,
    ProjectLocalForceAllTerminalRowForTest, ProjectLocalForceAllTypeKindForTest,
    ProjectLocalPublicationSccCommitForTest, ProjectLocalTerminalStateForTest,
};
use super::{
    ProjectLocalMappedProductGroupForTest, ProjectLocalProductDependencyForTest,
    ProjectLocalProductGroupForTest, ProjectLocalProductSccForTest,
};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

pub(super) struct ProjectLocalSemanticCoordinator {
    session: Rc<SessionCore>,
    #[cfg(any(test, feature = "test-utils"))]
    groups: Vec<ProjectLocalProductGroupForTest>,
    mapped_groups: Vec<ProjectLocalMappedProductGroupForTest>,
    #[cfg(any(test, feature = "test-utils"))]
    dependencies: Vec<ProjectLocalProductDependencyForTest>,
    dependency_targets: BTreeMap<usize, Vec<usize>>,
    containment_targets: BTreeMap<usize, Vec<usize>>,
    sccs: Vec<ProjectLocalProductSccForTest>,
    group_scc: BTreeMap<usize, usize>,
    #[cfg(any(test, feature = "test-utils"))]
    commits: Vec<ProjectLocalPublicationSccCommitForTest>,
    #[cfg(any(test, feature = "test-utils"))]
    cache_writes: Vec<ProjectLocalCacheWriteForTest>,
    semantic_group_types: Vec<Option<super::project_local_pass_port::TypeId>>,
    semantic_group_value_types: Vec<Option<super::project_local_pass_port::TypeId>>,
    ready: BTreeSet<usize>,
    value_ready: BTreeSet<usize>,
    type_parameter_ids: Vec<super::project_local_pass_port::TypeParamId>,
    type_parameter_defaults: Vec<super::project_local_pass_port::PublishedTypeParameterDefault>,
    aliases: PublishedAliases,
    pending_sccs: BTreeSet<usize>,
    materialized: BTreeSet<usize>,
    materialized_order: Vec<usize>,
    demand_roots: Vec<usize>,
    seen_demand_roots: BTreeSet<usize>,
    unavailable: BTreeSet<usize>,
    value_unavailable: BTreeSet<usize>,
    overlays: Vec<(TypeGroupId, ProjectLocalTypeOverlayDraft)>,
    library_records: LibraryRecordAccumulator,
    validations: ProjectLocalValidationAccumulator,
    product_lowering_metadata_plan: Option<ProductLoweringMetadataPlan>,
}

pub(super) struct ProjectLocalPassDemandGateway<'a, Commit> {
    coordinator: &'a mut ProjectLocalSemanticCoordinator,
    product: &'a LibraryDeclarationProduct,
    augmented_groups: &'a BTreeMap<usize, TypeGroupId>,
    native_array_groups: [Option<usize>; 2],
    type_products: BTreeMap<TypeGroupId, usize>,
    user_type_products: &'a BTreeMap<TypeGroupId, Vec<usize>>,
    value_products: BTreeMap<usize, usize>,
    commit: Commit,
}

impl<'a, Commit> ProjectLocalPassDemandGateway<'a, Commit> {
    fn new_inner(
        coordinator: &'a mut ProjectLocalSemanticCoordinator,
        product: &'a LibraryDeclarationProduct,
        augmented_groups: &'a BTreeMap<usize, TypeGroupId>,
        native_array_groups: [Option<usize>; 2],
        user_type_products: &'a BTreeMap<TypeGroupId, Vec<usize>>,
        commit: Commit,
    ) -> Self {
        let type_products = coordinator
            .mapped_groups
            .iter()
            .filter_map(|row| {
                if !row.product.has_type {
                    return None;
                }
                let local = u32::try_from(row.local_type_group?).ok()?;
                Some((TypeGroupId(local), row.product.product_key))
            })
            .collect();
        let value_products = coordinator
            .mapped_groups
            .iter()
            .filter_map(|row| {
                if !row.product.has_value && !row.product.has_namespace {
                    return None;
                }
                let local = u32::try_from(row.local_value?).ok()?;
                Some((usize::try_from(local).ok()?, row.product.product_key))
            })
            .collect();
        Self {
            coordinator,
            product,
            augmented_groups,
            native_array_groups,
            type_products,
            user_type_products,
            value_products,
            commit,
        }
    }

    pub(super) fn with_commit(
        coordinator: &'a mut ProjectLocalSemanticCoordinator,
        product: &'a LibraryDeclarationProduct,
        augmented_groups: &'a BTreeMap<usize, TypeGroupId>,
        native_array_groups: [Option<usize>; 2],
        user_type_products: &'a BTreeMap<TypeGroupId, Vec<usize>>,
        commit: Commit,
    ) -> Self {
        Self::new_inner(
            coordinator,
            product,
            augmented_groups,
            native_array_groups,
            user_type_products,
            commit,
        )
    }
}

impl<Commit> ProjectLocalPassDemandGateway<'_, Commit>
where
    Commit: FnMut(&mut ProjectLocalSemanticCoordinator) -> Result<(), &'static str>,
{
    fn demand(
        &mut self,
        interner: &mut super::project_local_pass_port::Interner,
        product_groups: &[usize],
    ) -> Result<ProjectLocalDemandSnapshot, &'static str> {
        if product_groups
            .iter()
            .all(|group| self.coordinator.session.cache().contains(*group))
        {
            for group in product_groups {
                self.verify_requested_ready(*group)?;
            }
            return self.snapshot();
        }
        self.coordinator.demand_product_groups_with_interner(
            self.product,
            interner,
            product_groups,
            self.augmented_groups,
            self.native_array_groups,
            #[cfg(any(test, feature = "test-utils"))]
            ProjectLocalSemanticPhaseForTest::Ordinary,
        );
        (self.commit)(self.coordinator)?;
        for group in product_groups {
            self.verify_requested_ready(*group)?;
        }
        self.snapshot()
    }

    fn verify_requested_ready(&self, product_group: usize) -> Result<(), &'static str> {
        match self.coordinator.session.cache().get(product_group) {
            Some(ProductTerminal::Ready { .. }) => Ok(()),
            Some(ProductTerminal::Unavailable(_)) => {
                Err("project-local demand published its requested terminal unavailable")
            }
            Some(ProductTerminal::Resolving) => {
                Err("project-local demand left its requested terminal resolving")
            }
            None => Err("project-local demand did not publish its requested terminal"),
        }
    }

    fn snapshot(&self) -> Result<ProjectLocalDemandSnapshot, &'static str> {
        self.coordinator.validate_publication_snapshot()?;
        let environment = project_local_published_types(
            self.product,
            &self.coordinator.mapped_groups,
            ProjectLocalPublishedTypeInputs {
                group_types: &self.coordinator.semantic_group_types,
                type_parameter_ids: &self.coordinator.type_parameter_ids,
                type_parameter_defaults: &self.coordinator.type_parameter_defaults,
                ready_product_groups: &self.coordinator.ready,
                unavailable_product_groups: &self.coordinator.unavailable,
                aliases: self.coordinator.aliases.clone(),
            },
        );
        let terminals = environment
            .product_parts()
            .map_err(|_| "project-local demand produced an incomplete type environment")?
            .groups;
        let type_terminals = self
            .coordinator
            .mapped_groups
            .iter()
            .filter(|row| {
                !self.augmented_groups.contains_key(&row.product.product_key)
                    && self
                        .coordinator
                        .session
                        .cache()
                        .contains(row.product.product_key)
            })
            .filter_map(|row| {
                let local = u32::try_from(row.local_type_group?).ok()?;
                let terminal = terminals.get(usize::try_from(local).ok()?)?.clone();
                Some((TypeGroupId(local), terminal))
            })
            .collect();
        let value_types = self
            .coordinator
            .mapped_groups
            .iter()
            .filter(|row| {
                !self.augmented_groups.contains_key(&row.product.product_key)
                    && self
                        .coordinator
                        .session
                        .cache()
                        .contains(row.product.product_key)
            })
            .filter_map(|row| {
                let storage = u32::try_from(row.local_value?).ok()?;
                let ty = self
                    .coordinator
                    .semantic_group_value_types
                    .get(row.product.product_key)
                    .copied()
                    .flatten()?;
                Some((ValueStorageId(storage), ty))
            })
            .collect();
        let namespace_seeds = self
            .coordinator
            .mapped_groups
            .iter()
            .filter(|row| {
                !self.augmented_groups.contains_key(&row.product.product_key)
                    && self
                        .coordinator
                        .session
                        .cache()
                        .contains(row.product.product_key)
            })
            .filter_map(|row| {
                let namespace = u32::try_from(row.local_namespace?).ok()?;
                let storage = u32::try_from(row.local_value?).ok()?;
                let ty = self
                    .coordinator
                    .semantic_group_value_types
                    .get(row.product.product_key)
                    .copied()
                    .flatten()?;
                Some(super::project_local_pass_port::ProjectLocalNamespaceSeed {
                    namespace: super::project_local_pass_port::NamespaceId(namespace),
                    storage: ValueStorageId(storage),
                    ty,
                })
            })
            .collect();
        Ok(ProjectLocalDemandSnapshot {
            aliases: self.coordinator.aliases.clone(),
            type_terminals,
            value_types,
            namespace_seeds,
        })
    }
}

impl<Commit> ProjectLocalUserDemandGateway for ProjectLocalPassDemandGateway<'_, Commit>
where
    Commit: FnMut(&mut ProjectLocalSemanticCoordinator) -> Result<(), &'static str>,
{
    fn demand_type_group(
        &mut self,
        interner: &mut super::project_local_pass_port::Interner,
        group: TypeGroupId,
    ) -> Result<ProjectLocalDemandSnapshot, &'static str> {
        if let Some(product_group) = self.type_products.get(&group).copied() {
            return self.demand(interner, &[product_group]);
        }
        let Some(product_groups) = self.user_type_products.get(&group).cloned() else {
            return Ok(empty_demand_snapshot());
        };
        self.demand(interner, &product_groups)
    }

    fn demand_value(
        &mut self,
        interner: &mut super::project_local_pass_port::Interner,
        storage: ValueStorageId,
    ) -> Result<ProjectLocalDemandSnapshot, &'static str> {
        let Some(product_group) = self.value_products.get(&storage.index()).copied() else {
            return Ok(empty_demand_snapshot());
        };
        self.demand(interner, &[product_group])
    }

    fn commit_staged(&mut self) -> Result<ProjectLocalDemandSnapshot, &'static str> {
        (self.commit)(self.coordinator)?;
        self.snapshot()
    }
}

fn empty_demand_snapshot() -> ProjectLocalDemandSnapshot {
    ProjectLocalDemandSnapshot {
        aliases: PublishedAliases::empty(),
        type_terminals: Vec::new(),
        value_types: Vec::new(),
        namespace_seeds: Vec::new(),
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PublicationInjection {
    None,
    PublishBeforeSccComplete,
    WriteResolvingToDurableCache,
}

trait PublicationFaultPolicy {
    fn publish_before_scc_complete(&mut self, member_count: usize) -> bool;
    fn write_resolving_to_durable_cache(&mut self) -> bool;
    fn record_partial_publication(&mut self, scc: usize);
    fn should_stop(&self) -> bool;
}

struct OrdinaryPublication;

impl PublicationFaultPolicy for OrdinaryPublication {
    fn publish_before_scc_complete(&mut self, _member_count: usize) -> bool {
        false
    }

    fn write_resolving_to_durable_cache(&mut self) -> bool {
        false
    }

    fn record_partial_publication(&mut self, _scc: usize) {}

    fn should_stop(&self) -> bool {
        false
    }
}

#[cfg(any(test, feature = "test-utils"))]
struct InjectedPublication<Activate> {
    injection: PublicationInjection,
    reached: bool,
    stop: bool,
    activate: Activate,
}

#[cfg(any(test, feature = "test-utils"))]
impl<Activate> InjectedPublication<Activate> {
    fn new(injection: PublicationInjection, activate: Activate) -> Self {
        Self {
            injection,
            reached: false,
            stop: false,
            activate,
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl<Activate: FnMut(PublicationInjection)> PublicationFaultPolicy
    for InjectedPublication<Activate>
{
    fn publish_before_scc_complete(&mut self, member_count: usize) -> bool {
        let reached = !self.reached
            && member_count > 1
            && self.injection == PublicationInjection::PublishBeforeSccComplete;
        if reached {
            self.reached = true;
            (self.activate)(self.injection);
        }
        reached
    }

    fn write_resolving_to_durable_cache(&mut self) -> bool {
        let reached =
            !self.reached && self.injection == PublicationInjection::WriteResolvingToDurableCache;
        if reached {
            self.reached = true;
            (self.activate)(self.injection);
        }
        reached
    }

    fn record_partial_publication(&mut self, _scc: usize) {
        self.stop = true;
    }

    fn should_stop(&self) -> bool {
        self.stop
    }
}

impl ProjectLocalSemanticCoordinator {
    pub(super) fn new_ordinary(
        product: &LibraryDeclarationProduct,
        mapped_groups: Vec<ProjectLocalMappedProductGroupForTest>,
        session: Rc<SessionCore>,
    ) -> Self {
        let dependencies = product_dependencies(product);
        Self::new_inner(product, mapped_groups, session, dependencies)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn new_with_dependencies(
        product: &LibraryDeclarationProduct,
        mapped_groups: Vec<ProjectLocalMappedProductGroupForTest>,
        session: Rc<SessionCore>,
        dependencies: Vec<ProjectLocalProductDependencyForTest>,
    ) -> Self {
        Self::new_inner(product, mapped_groups, session, dependencies)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn dependency_rows(
        product: &LibraryDeclarationProduct,
    ) -> Vec<ProjectLocalProductDependencyForTest> {
        product_dependencies(product)
    }

    fn new_inner(
        product: &LibraryDeclarationProduct,
        mapped_groups: Vec<ProjectLocalMappedProductGroupForTest>,
        session: Rc<SessionCore>,
        dependencies: Vec<ProjectLocalProductDependencyForTest>,
    ) -> Self {
        let groups = product
            .groups()
            .iter()
            .enumerate()
            .map(|(product_key, group)| ProjectLocalProductGroupForTest {
                product_key,
                container_key: group.container.index(),
                compilation_global: product.containers()[group.container.index()].kind
                    == ContainerKind::CompilationGlobal,
                name: {
                    #[cfg(test)]
                    super::contract::record_project_local_product_group_name_copy_for_test();
                    product.strings()[group.name.index()].clone()
                },
                has_type: group.slots.r#type,
                has_value: group.slots.value,
                has_namespace: group.slots.namespace,
            })
            .collect::<Vec<_>>();
        let mut dependency_targets = BTreeMap::<usize, Vec<usize>>::new();
        for row in &dependencies {
            dependency_targets
                .entry(row.owner)
                .or_default()
                .push(row.target);
        }
        let containment_targets = product
            .groups()
            .iter()
            .enumerate()
            .filter_map(|(owner, group)| {
                let children = group
                    .fragments
                    .iter()
                    .filter_map(|declaration| {
                        match product.declarations()[declaration.index()].payload {
                            DeclarationPayload::Namespace { child_region }
                            | DeclarationPayload::Global { child_region } => {
                                Some(product.regions()[child_region.index()].container)
                            }
                            _ => None,
                        }
                    })
                    .flat_map(|container| {
                        product
                            .groups()
                            .iter()
                            .enumerate()
                            .filter_map(move |(index, child)| {
                                (child.container == container
                                    && (child.slots.namespace
                                        || group_has_runtime_value(product, child)))
                                .then_some(index)
                            })
                    })
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
                (!children.is_empty()).then_some((owner, children))
            })
            .collect();
        let sccs = product
            .dependency_sccs()
            .into_iter()
            .map(|members| ProjectLocalProductSccForTest {
                members: members
                    .into_iter()
                    .map(|key| groups[key.index()].clone())
                    .collect(),
            })
            .collect::<Vec<_>>();
        let group_scc = sccs
            .iter()
            .enumerate()
            .flat_map(|(scc, row)| {
                row.members
                    .iter()
                    .map(move |member| (member.product_key, scc))
            })
            .collect();
        Self {
            session,
            #[cfg(any(test, feature = "test-utils"))]
            groups,
            mapped_groups,
            #[cfg(any(test, feature = "test-utils"))]
            dependencies,
            dependency_targets,
            containment_targets,
            sccs,
            group_scc,
            #[cfg(any(test, feature = "test-utils"))]
            commits: Vec::new(),
            #[cfg(any(test, feature = "test-utils"))]
            cache_writes: Vec::new(),
            semantic_group_types: vec![None; product.group_count()],
            semantic_group_value_types: vec![None; product.group_count()],
            ready: BTreeSet::new(),
            value_ready: BTreeSet::new(),
            type_parameter_ids: (0..product.type_parameters().len())
                .map(|index| {
                    TypeParamId(
                        u32::try_from(index).expect("product type-parameter identity fits u32"),
                    )
                })
                .collect(),
            type_parameter_defaults: product
                .type_parameters()
                .iter()
                .map(|parameter| {
                    if parameter.default.is_some() {
                        super::project_local_pass_port::PublishedTypeParameterDefault::Unsupported
                    } else {
                        super::project_local_pass_port::PublishedTypeParameterDefault::Absent
                    }
                })
                .collect(),
            aliases: PublishedAliases::empty(),
            pending_sccs: BTreeSet::new(),
            materialized: BTreeSet::new(),
            materialized_order: Vec::new(),
            demand_roots: Vec::new(),
            seen_demand_roots: BTreeSet::new(),
            unavailable: BTreeSet::new(),
            value_unavailable: BTreeSet::new(),
            overlays: Vec::new(),
            library_records: LibraryRecordAccumulator::default(),
            validations: ProjectLocalValidationAccumulator::default(),
            product_lowering_metadata_plan: None,
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn groups(&self) -> &[ProjectLocalProductGroupForTest] {
        &self.groups
    }

    pub(super) fn mapped_groups(&self) -> &[ProjectLocalMappedProductGroupForTest] {
        &self.mapped_groups
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn dependencies(&self) -> &[ProjectLocalProductDependencyForTest] {
        &self.dependencies
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn sccs(&self) -> &[ProjectLocalProductSccForTest] {
        &self.sccs
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn commits(&self) -> &[ProjectLocalPublicationSccCommitForTest] {
        &self.commits
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn cache_writes(&self) -> &[ProjectLocalCacheWriteForTest] {
        &self.cache_writes
    }

    pub(super) fn demand_product_groups(
        &mut self,
        product: &LibraryDeclarationProduct,
        roots: &[usize],
        augmented_groups: &BTreeMap<usize, TypeGroupId>,
        native_array_groups: [Option<usize>; 2],
    ) -> Vec<usize> {
        let session = Rc::clone(&self.session);
        session.with_interner(|interner| {
            self.demand_product_groups_with_interner(
                product,
                interner,
                roots,
                augmented_groups,
                native_array_groups,
                #[cfg(any(test, feature = "test-utils"))]
                ProjectLocalSemanticPhaseForTest::Ordinary,
            )
        })
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn demand_product_groups_for_phase(
        &mut self,
        product: &LibraryDeclarationProduct,
        roots: &[usize],
        augmented_groups: &BTreeMap<usize, TypeGroupId>,
        native_array_groups: [Option<usize>; 2],
        phase: ProjectLocalSemanticPhaseForTest,
    ) -> Vec<usize> {
        let session = Rc::clone(&self.session);
        session.with_interner(|interner| {
            self.demand_product_groups_with_interner(
                product,
                interner,
                roots,
                augmented_groups,
                native_array_groups,
                phase,
            )
        })
    }

    pub(super) fn demand_product_groups_with_interner(
        &mut self,
        product: &LibraryDeclarationProduct,
        interner: &mut super::project_local_pass_port::Interner,
        roots: &[usize],
        augmented_groups: &BTreeMap<usize, TypeGroupId>,
        native_array_groups: [Option<usize>; 2],
        #[cfg(any(test, feature = "test-utils"))] phase: ProjectLocalSemanticPhaseForTest,
    ) -> Vec<usize> {
        for root in roots {
            if self.seen_demand_roots.insert(*root) {
                self.demand_roots.push(*root);
            }
        }
        let mut closure = self.closure(roots);
        if closure.is_empty() {
            return Vec::new();
        }
        if closure
            .iter()
            .all(|group| self.session.cache().contains(*group))
        {
            for root in roots {
                let _ = self.session.cache().get(*root);
            }
            return closure.into_iter().collect();
        }
        if self.product_lowering_metadata_plan.is_none() {
            self.product_lowering_metadata_plan = Some(ProductLoweringMetadataPlan::new(product));
        }
        let lowering = loop {
            for product_group in &closure {
                if !self.session.cache().contains(*product_group)
                    && self.materialized.insert(*product_group)
                {
                    self.materialized_order.push(*product_group);
                    #[cfg(any(test, feature = "test-utils"))]
                    ProductTerminal::materialize(phase, *product_group);
                }
            }
            let materialized = closure.iter().copied().collect::<Vec<_>>();
            let seed = ProductLoweringSeed {
                group_types: &self.semantic_group_types,
                group_value_types: &self.semantic_group_value_types,
                ready: &self.ready,
                value_ready: &self.value_ready,
                unavailable: &self.unavailable,
                value_unavailable: &self.value_unavailable,
                type_parameter_defaults: &self.type_parameter_defaults,
                aliases: &self.aliases,
                overlays: &self.overlays,
            };
            let lowering = lower_product_groups(
                product,
                self.product_lowering_metadata_plan
                    .as_ref()
                    .expect("nonempty uncached lowering constructs its metadata plan"),
                interner,
                &materialized,
                augmented_groups,
                native_array_groups,
                Some(seed),
            );
            if lowering.missing_admissions.is_empty() {
                break lowering;
            }
            let mut expanded_roots = closure.iter().copied().collect::<Vec<_>>();
            expanded_roots.extend(lowering.missing_admissions.iter().copied());
            let expanded = self.closure(&expanded_roots);
            if expanded == closure {
                break lowering;
            }
            closure = expanded;
        };
        let needed_sccs = closure
            .iter()
            .filter_map(|group| self.group_scc.get(group).copied())
            .collect::<BTreeSet<_>>();
        if !lowering.missing_admissions.is_empty() {
            return closure.into_iter().collect();
        }
        self.library_records.extend(lowering.library_record_events);
        self.validations.extend(lowering.validation_events);
        self.semantic_group_types = lowering.group_types.clone();
        self.semantic_group_value_types = lowering.group_value_types.clone();
        self.ready = lowering.ready;
        self.value_ready = lowering.value_ready;
        self.type_parameter_ids = lowering.type_parameter_ids.clone();
        self.type_parameter_defaults = lowering.type_parameter_defaults.clone();
        self.aliases = lowering.aliases;
        self.pending_sccs.extend(needed_sccs);
        self.unavailable = lowering.unavailable;
        self.value_unavailable = lowering.value_unavailable;
        self.overlays = lowering.overlays;
        closure.into_iter().collect()
    }

    pub(super) fn overlays(&self) -> &[(TypeGroupId, ProjectLocalTypeOverlayDraft)] {
        &self.overlays
    }

    pub(super) fn materialized(&self) -> Vec<usize> {
        self.materialized_order.clone()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn demand_roots(&self) -> &[usize] {
        &self.demand_roots
    }

    fn commit_demanded<Policy: PublicationFaultPolicy>(&mut self, policy: &mut Policy) {
        let pending_sccs = self.pending_sccs.clone();
        self.pending_sccs.clear();
        for scc in pending_sccs {
            let group_types = self.semantic_group_types.clone();
            let group_value_types = self.semantic_group_value_types.clone();
            let mut unavailable = self.unavailable.clone();
            unavailable.extend(self.value_unavailable.iter().copied());
            self.commit_component(scc, &group_types, &group_value_types, &unavailable, policy);
            if policy.should_stop() {
                break;
            }
        }
    }

    pub(super) fn commit_demanded_checked(&mut self) -> Result<(), &'static str> {
        let mut policy = OrdinaryPublication;
        self.commit_demanded_checked_inner(&mut policy)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn commit_demanded_checked_with_injection<Activate>(
        &mut self,
        injection: PublicationInjection,
        activate: Activate,
    ) -> Result<(), &'static str>
    where
        Activate: FnMut(PublicationInjection),
    {
        let mut policy = InjectedPublication::new(injection, activate);
        self.commit_demanded_checked_inner(&mut policy)
    }

    fn commit_demanded_checked_inner<Policy: PublicationFaultPolicy>(
        &mut self,
        policy: &mut Policy,
    ) -> Result<(), &'static str> {
        self.validate_publication_snapshot()?;
        self.commit_demanded(policy);
        self.validate_publication_snapshot()
    }

    fn validate_publication_snapshot(&self) -> Result<(), &'static str> {
        for scc in &self.sccs {
            let published = scc
                .members
                .iter()
                .filter(|member| self.session.cache().contains(member.product_key))
                .count();
            if published != 0 && published != scc.members.len() {
                return Err("project-local demand observed a partial publication SCC");
            }
        }
        Ok(())
    }

    pub(super) fn semantic_group_types(&self) -> &[Option<super::project_local_pass_port::TypeId>] {
        &self.semantic_group_types
    }

    pub(super) fn semantic_group_value_types(
        &self,
    ) -> &[Option<super::project_local_pass_port::TypeId>] {
        &self.semantic_group_value_types
    }

    pub(super) fn type_parameter_ids(&self) -> &[super::project_local_pass_port::TypeParamId] {
        &self.type_parameter_ids
    }

    pub(super) fn type_parameter_defaults(
        &self,
    ) -> &[super::project_local_pass_port::PublishedTypeParameterDefault] {
        &self.type_parameter_defaults
    }

    pub(super) fn aliases(&self) -> &PublishedAliases {
        &self.aliases
    }

    pub(super) fn unavailable(&self) -> &BTreeSet<usize> {
        &self.unavailable
    }

    pub(super) fn ready(&self) -> &BTreeSet<usize> {
        &self.ready
    }

    pub(super) fn validation_owner_closure(
        &self,
        changed_groups: impl IntoIterator<Item = usize>,
        dependencies: &[(usize, usize)],
    ) -> Vec<usize> {
        let mut dependents = BTreeMap::<usize, Vec<usize>>::new();
        for (owner, target) in dependencies {
            dependents.entry(*target).or_default().push(*owner);
        }
        let mut roots = BTreeSet::new();
        let mut pending = changed_groups.into_iter().collect::<Vec<_>>();
        while let Some(group) = pending.pop() {
            if !roots.insert(group) {
                continue;
            }
            pending.extend(dependents.get(&group).into_iter().flatten().copied());
        }
        roots.into_iter().collect()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn ready_groups(&self) -> Vec<usize> {
        self.ready.iter().copied().collect()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn library_records(&self) -> Vec<super::ProjectLocalLibraryRecordForTest> {
        self.library_records.normalized()
    }

    pub(super) fn validations_for(
        &self,
        ready_groups: impl IntoIterator<Item = usize>,
        visited_groups: impl IntoIterator<Item = usize>,
    ) -> Vec<ProjectLocalValidationCandidate> {
        let ready_groups = ready_groups.into_iter().collect::<BTreeSet<_>>();
        let visited_groups = visited_groups.into_iter().collect::<BTreeSet<_>>();
        self.validations
            .candidates_for(&ready_groups, &visited_groups)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn repeat(&self, product_group: usize) -> Option<String> {
        self.session
            .cache()
            .peek(product_group)
            .map(|terminal| terminal.identity())
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn force_all_terminal_census(
        &self,
        interner: &Interner,
    ) -> Result<Vec<ProjectLocalForceAllTerminalRowForTest>, &'static str> {
        self.groups
            .iter()
            .map(|group| {
                let terminal = match self.session.cache().peek(group.product_key) {
                    Some(ProductTerminal::Ready { ty, value_ty, .. }) => {
                        let type_surface = ty
                            .map(|ty| self.force_all_ready_surface(interner, ty, true))
                            .transpose()?;
                        let value_surface = value_ty
                            .map(|ty| self.force_all_ready_surface(interner, ty, true))
                            .transpose()?;
                        ProjectLocalForceAllTerminalForTest::Ready {
                            type_surface,
                            value_surface,
                        }
                    }
                    Some(ProductTerminal::Unavailable(reason)) => {
                        ProjectLocalForceAllTerminalForTest::Unavailable(reason)
                    }
                    Some(ProductTerminal::Resolving) | None => {
                        return Err("force_all terminal census observed an unpublished group");
                    }
                };
                Ok(ProjectLocalForceAllTerminalRowForTest {
                    group: group.clone(),
                    terminal,
                })
            })
            .collect()
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn force_all_ready_surface(
        &self,
        interner: &Interner,
        ty: TypeId,
        expand_alias: bool,
    ) -> Result<ProjectLocalForceAllReadySurfaceForTest, &'static str> {
        let store = interner.store();
        if let Some(object) = store.object_type(ty) {
            return Ok(ProjectLocalForceAllReadySurfaceForTest::Object(
                ProjectLocalForceAllObjectShapeForTest {
                    property_count: object.properties.len(),
                    call_signature_count: object.call_signatures.len(),
                    construct_signature_count: object.construct_signatures.len(),
                    has_string_index: object.string_index.is_some(),
                    has_number_index: object.number_index.is_some(),
                },
            ));
        }
        if expand_alias && store.tag(ty) == TypeTag::AliasTemplate {
            let alias = store
                .alias_template_id(ty)
                .ok_or("force_all alias surface has no authenticated alias identity")?;
            let terminal = match self
                .aliases
                .terminal(alias)
                .ok_or("force_all alias surface has no published terminal")?
            {
                PublishedAliasTerminal::Ready(surface) => {
                    if surface.alias() != alias || surface.template() != ty {
                        return Err("force_all alias terminal does not match its live template");
                    }
                    ProjectLocalForceAllAliasTerminalForTest::Ready {
                        parameter_count: surface.type_params().len(),
                        body: Box::new(self.force_all_ready_surface(
                            interner,
                            surface.body(),
                            false,
                        )?),
                    }
                }
                PublishedAliasTerminal::Unavailable(reason) => {
                    ProjectLocalForceAllAliasTerminalForTest::Unavailable(match reason {
                        PublishedAliasUnavailable::DirectCycle => {
                            ProjectLocalForceAllAliasUnavailableForTest::DirectCycle
                        }
                        PublishedAliasUnavailable::UnsupportedBody => {
                            ProjectLocalForceAllAliasUnavailableForTest::UnsupportedBody
                        }
                        PublishedAliasUnavailable::ComponentFailure => {
                            ProjectLocalForceAllAliasUnavailableForTest::ComponentFailure
                        }
                    })
                }
            };
            return Ok(ProjectLocalForceAllReadySurfaceForTest::Alias(terminal));
        }
        Ok(ProjectLocalForceAllReadySurfaceForTest::Other(
            force_all_type_kind(interner, ty),
        ))
    }

    fn closure(&self, roots: &[usize]) -> BTreeSet<usize> {
        let mut closure = BTreeSet::new();
        let mut pending = roots.to_vec();
        while let Some(owner) = pending.pop() {
            let Some(scc) = self.group_scc.get(&owner).copied() else {
                continue;
            };
            for member in &self.sccs[scc].members {
                if closure.insert(member.product_key) {
                    pending.extend(
                        self.dependency_targets
                            .get(&member.product_key)
                            .into_iter()
                            .flatten()
                            .copied(),
                    );
                    pending.extend(
                        self.containment_targets
                            .get(&member.product_key)
                            .into_iter()
                            .flatten()
                            .copied(),
                    );
                }
            }
        }
        closure
    }

    fn commit_component<Policy: PublicationFaultPolicy>(
        &mut self,
        scc_index: usize,
        group_types: &[Option<super::project_local_pass_port::TypeId>],
        group_value_types: &[Option<super::project_local_pass_port::TypeId>],
        unavailable: &BTreeSet<usize>,
        policy: &mut Policy,
    ) {
        let scc = self.sccs[scc_index].clone();
        if scc
            .members
            .iter()
            .all(|member| self.session.cache().contains(member.product_key))
        {
            return;
        }
        let component_unavailable = scc
            .members
            .iter()
            .any(|member| unavailable.contains(&member.product_key));
        let premature = policy.publish_before_scc_complete(scc.members.len());
        if premature {
            policy.record_partial_publication(scc_index);
            let member = scc
                .members
                .first()
                .expect("non-trivial publication SCC has a first member")
                .clone();
            let mut identity = "publisher:".to_owned();
            identity.push_str(&self.session.publisher_identity().to_string());
            identity.push_str(":product:");
            identity.push_str(&member.product_key.to_string());
            if self.session.cache().commit(
                member.product_key,
                ProductTerminal::Ready {
                    identity,
                    ty: group_types[member.product_key],
                    value_ty: group_value_types[member.product_key],
                },
            ) {
                #[cfg(any(test, feature = "test-utils"))]
                self.cache_writes.push(ProjectLocalCacheWriteForTest {
                    owner: member.clone(),
                    state: ProjectLocalTerminalStateForTest::Ready,
                });
            }
            #[cfg(any(test, feature = "test-utils"))]
            self.commits.push(ProjectLocalPublicationSccCommitForTest {
                scc: scc.clone(),
                reserved_members: scc.members,
                published_members: vec![member],
                observed_before_complete: Vec::new(),
                state_before_publish: ProjectLocalTerminalStateForTest::Resolving,
                final_state: ProjectLocalTerminalStateForTest::Ready,
            });
            return;
        }
        let resolving_cache = policy.write_resolving_to_durable_cache();
        let mut published = Vec::with_capacity(scc.members.len());
        for member in &scc.members {
            let terminal = if component_unavailable {
                let reason = super::ProjectLocalUnavailableReasonForTest::UnsupportedProductRecipe;
                ProductTerminal::Unavailable(reason)
            } else {
                let mut identity = "publisher:".to_owned();
                identity.push_str(&self.session.publisher_identity().to_string());
                identity.push_str(":product:");
                identity.push_str(&member.product_key.to_string());
                ProductTerminal::Ready {
                    identity,
                    ty: group_types[member.product_key],
                    value_ty: group_value_types[member.product_key],
                }
            };
            let terminal = if resolving_cache && published.is_empty() {
                ProductTerminal::Resolving
            } else {
                terminal
            };
            #[cfg(any(test, feature = "test-utils"))]
            let state = terminal_state_for_test(&terminal);
            if self.session.cache().commit(member.product_key, terminal) {
                #[cfg(any(test, feature = "test-utils"))]
                self.cache_writes.push(ProjectLocalCacheWriteForTest {
                    owner: member.clone(),
                    state,
                });
            }
            if !component_unavailable {
                published.push(member.clone());
            }
        }
        #[cfg(any(test, feature = "test-utils"))]
        self.commits.push(ProjectLocalPublicationSccCommitForTest {
            scc: scc.clone(),
            reserved_members: scc.members,
            published_members: published,
            observed_before_complete: Vec::new(),
            state_before_publish: ProjectLocalTerminalStateForTest::Resolving,
            final_state: if component_unavailable {
                ProjectLocalTerminalStateForTest::Unavailable(
                    super::ProjectLocalUnavailableReasonForTest::UnsupportedProductRecipe,
                )
            } else {
                ProjectLocalTerminalStateForTest::Ready
            },
        });
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn terminal_state_for_test(terminal: &ProductTerminal) -> ProjectLocalTerminalStateForTest {
    match terminal {
        ProductTerminal::Resolving => ProjectLocalTerminalStateForTest::Resolving,
        ProductTerminal::Ready { .. } => ProjectLocalTerminalStateForTest::Ready,
        ProductTerminal::Unavailable(reason) => {
            ProjectLocalTerminalStateForTest::Unavailable(*reason)
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn force_all_type_kind(interner: &Interner, ty: TypeId) -> ProjectLocalForceAllTypeKindForTest {
    let store = interner.store();
    match store.tag(ty) {
        TypeTag::Intrinsic => ProjectLocalForceAllTypeKindForTest::Intrinsic(
            match store
                .intrinsic_kind(ty)
                .expect("intrinsic type has an intrinsic kind")
            {
                IntrinsicKind::Error => ProjectLocalForceAllIntrinsicKindForTest::Error,
                IntrinsicKind::Any => ProjectLocalForceAllIntrinsicKindForTest::Any,
                IntrinsicKind::Unknown => ProjectLocalForceAllIntrinsicKindForTest::Unknown,
                IntrinsicKind::Never => ProjectLocalForceAllIntrinsicKindForTest::Never,
                IntrinsicKind::Void => ProjectLocalForceAllIntrinsicKindForTest::Void,
                IntrinsicKind::Null => ProjectLocalForceAllIntrinsicKindForTest::Null,
                IntrinsicKind::Undefined => ProjectLocalForceAllIntrinsicKindForTest::Undefined,
                IntrinsicKind::Boolean => ProjectLocalForceAllIntrinsicKindForTest::Boolean,
                IntrinsicKind::Number => ProjectLocalForceAllIntrinsicKindForTest::Number,
                IntrinsicKind::String => ProjectLocalForceAllIntrinsicKindForTest::String,
                IntrinsicKind::Uppercase => ProjectLocalForceAllIntrinsicKindForTest::Uppercase,
                IntrinsicKind::Lowercase => ProjectLocalForceAllIntrinsicKindForTest::Lowercase,
                IntrinsicKind::Capitalize => ProjectLocalForceAllIntrinsicKindForTest::Capitalize,
                IntrinsicKind::Uncapitalize => {
                    ProjectLocalForceAllIntrinsicKindForTest::Uncapitalize
                }
                IntrinsicKind::ThisType => ProjectLocalForceAllIntrinsicKindForTest::ThisType,
                IntrinsicKind::OmitThisParameter => {
                    ProjectLocalForceAllIntrinsicKindForTest::OmitThisParameter
                }
                IntrinsicKind::Object => ProjectLocalForceAllIntrinsicKindForTest::Object,
                IntrinsicKind::BigInt => ProjectLocalForceAllIntrinsicKindForTest::BigInt,
                IntrinsicKind::Symbol => ProjectLocalForceAllIntrinsicKindForTest::Symbol,
            },
        ),
        TypeTag::Literal => ProjectLocalForceAllTypeKindForTest::Literal(
            match store
                .literal_value(ty)
                .expect("literal type has a literal value")
            {
                LiteralValue::Number(_) => ProjectLocalForceAllLiteralKindForTest::Number,
                LiteralValue::String(_) => ProjectLocalForceAllLiteralKindForTest::String,
                LiteralValue::Boolean(_) => ProjectLocalForceAllLiteralKindForTest::Boolean,
                LiteralValue::WellKnownSymbol(_) => {
                    ProjectLocalForceAllLiteralKindForTest::WellKnownSymbol
                }
            },
        ),
        TypeTag::Object => ProjectLocalForceAllTypeKindForTest::Object,
        TypeTag::Union => ProjectLocalForceAllTypeKindForTest::Union,
        TypeTag::Intersection => ProjectLocalForceAllTypeKindForTest::Intersection,
        TypeTag::Function => ProjectLocalForceAllTypeKindForTest::Function,
        TypeTag::TypeParam => ProjectLocalForceAllTypeKindForTest::TypeParameter,
        TypeTag::Array => ProjectLocalForceAllTypeKindForTest::Array,
        TypeTag::Tuple => ProjectLocalForceAllTypeKindForTest::Tuple,
        TypeTag::Readonly => ProjectLocalForceAllTypeKindForTest::Readonly,
        TypeTag::Conditional => ProjectLocalForceAllTypeKindForTest::Conditional,
        TypeTag::Instantiation => ProjectLocalForceAllTypeKindForTest::Instantiation,
        TypeTag::Infer => ProjectLocalForceAllTypeKindForTest::Infer,
        TypeTag::Mapped => ProjectLocalForceAllTypeKindForTest::Mapped,
        TypeTag::MappedValue => ProjectLocalForceAllTypeKindForTest::MappedValue,
        TypeTag::Template => ProjectLocalForceAllTypeKindForTest::Template,
        TypeTag::Keyof => ProjectLocalForceAllTypeKindForTest::Keyof,
        TypeTag::ClassInstance => ProjectLocalForceAllTypeKindForTest::ClassInstance,
        TypeTag::DeferredIndexedAccess => {
            ProjectLocalForceAllTypeKindForTest::DeferredIndexedAccess
        }
        TypeTag::Declared => ProjectLocalForceAllTypeKindForTest::Declared,
        TypeTag::Enum => ProjectLocalForceAllTypeKindForTest::Enum,
        TypeTag::EnumMember => ProjectLocalForceAllTypeKindForTest::EnumMember,
        TypeTag::AliasTemplate => ProjectLocalForceAllTypeKindForTest::AliasTemplate,
    }
}

fn product_dependencies(
    product: &LibraryDeclarationProduct,
) -> Vec<ProjectLocalProductDependencyForTest> {
    let mut dependencies = product
        .dependency_rows()
        .iter()
        .filter_map(|row| {
            let DependencyTarget::Group(target) = row.target else {
                return None;
            };
            let source = &product.sources()[row.site.source.index()];
            Some(ProjectLocalProductDependencyForTest {
                owner: row.owner.index(),
                target: target.index(),
                kind: dependency_kind_name(row.kind).to_owned(),
                source_path: product.strings()[source.name.index()].clone(),
                source_start: row.site.start,
            })
        })
        .collect::<Vec<_>>();
    dependencies.sort();
    dependencies
}

const fn dependency_kind_name(kind: DependencyKind) -> &'static str {
    match kind {
        DependencyKind::Reference => "Reference",
        DependencyKind::Heritage => "Heritage",
        DependencyKind::Query => "Query",
        DependencyKind::Import => "Import",
        DependencyKind::IndexedAccess => "IndexedAccess",
        DependencyKind::Constraint => "Constraint",
    }
}

fn group_has_runtime_value(
    product: &LibraryDeclarationProduct,
    group: &super::project_local_pass_port::DeclarationGroupRecipe,
) -> bool {
    group.fragments.iter().any(|declaration| {
        matches!(
            product.declarations()[declaration.index()].payload,
            DeclarationPayload::Variable { .. }
                | DeclarationPayload::Function { .. }
                | DeclarationPayload::Class { .. }
                | DeclarationPayload::Enum { .. }
                | DeclarationPayload::Namespace { .. }
                | DeclarationPayload::Global { .. }
        )
    })
}

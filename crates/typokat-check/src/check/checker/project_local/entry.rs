//! Capability-limited entry for the project-local semantic candidate.

#[cfg(any(test, feature = "test-utils"))]
use super::coordinator::PublicationInjection;
use super::coordinator::{ProjectLocalPassDemandGateway, ProjectLocalSemanticCoordinator};
#[cfg(any(test, feature = "test-utils"))]
use super::fault_gate::FaultStateForTest;
#[cfg(any(test, feature = "test-utils"))]
use super::global_owner::publish_final_global_contributors;
#[cfg(any(test, feature = "test-utils"))]
use super::lowerer::{build_semantic_probes, SemanticProbeInput};
#[cfg(any(test, feature = "test-utils"))]
use super::observation::ProjectLocalSemanticPhaseForTest;
use super::project_local_pass_port::{
    project_local_final_type_dependencies, project_local_published_types,
    project_local_user_augmented_product_groups, run_project_local_user_pass, BoundProjectBinder,
    CheckResult, ContainerKind, LibraryBinderUnit, LibraryDeclarationProduct, NamespaceId,
    ProjectLocalFinalTypeDependencies, ProjectLocalNamespaceSeed, ProjectLocalPublishedTypeInputs,
    ProjectLocalUserPassOutput, ProjectLocalUserPassRequest, ProjectProgram, TypeGroupId,
};
#[cfg(any(test, feature = "test-utils"))]
use super::project_local_pass_port::{
    project_local_final_type_group_fragment_counts, project_local_final_value_declaration_counts,
    project_local_probe_authority, project_local_semantic_query_state,
    project_local_user_pass_observables, run_project_local_user_pass_for_test, sha256_hex,
};
use super::session::SessionCore;
use super::ProjectLocalMappedProductGroupForTest;
#[cfg(any(test, feature = "test-utils"))]
use super::{
    ProjectLocalRepeatedDemandForTest, ProjectLocalSccForTest, ProjectLocalSccPublicationForTest,
    ProjectLocalSemanticRequestForTest, ProjectLocalSemanticTraceForTest,
    ProjectLocalSourceInputForTest, ProjectLocalTerminalStateForTest,
};
#[cfg(any(test, feature = "test-utils"))]
use std::collections::BTreeSet;
use std::fmt;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectLocalCandidateErrorForTest {
    #[cfg(any(test, feature = "test-utils"))]
    NotImplemented,
    MissingProductBinderMap,
    MissingMappedProductGroup(usize),
    #[cfg(any(test, feature = "test-utils"))]
    GlobalObject(&'static str),
    UserPass(&'static str),
}

impl fmt::Display for ProjectLocalCandidateErrorForTest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(any(test, feature = "test-utils"))]
            Self::NotImplemented => {
                formatter.write_str("WU4 project-local semantic coordinator is not implemented")
            }
            Self::MissingProductBinderMap => {
                formatter.write_str("project-local binder has no immutable-product identity map")
            }
            Self::MissingMappedProductGroup(group) => {
                let mut message = "product group ".to_owned();
                message.push_str(&group.to_string());
                message.push_str(" has no project-local identity");
                formatter.write_str(&message)
            }
            #[cfg(any(test, feature = "test-utils"))]
            Self::GlobalObject(message) => formatter.write_str(message),
            Self::UserPass(message) => formatter.write_str(message),
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub fn project_local_semantic_candidate_for_test<'ast>(
    product: &LibraryDeclarationProduct,
    library_units: Vec<LibraryBinderUnit>,
    bound: BoundProjectBinder,
    programs: &[ProjectProgram<'ast>],
    request: ProjectLocalSemanticRequestForTest,
) -> Result<ProjectLocalSemanticTraceForTest, ProjectLocalCandidateErrorForTest> {
    let prepared = prepare_project_local_execution_for_test(
        product,
        library_units,
        bound,
        programs,
        &request.demand_roots,
        request.fault,
        request.force_all,
    )?;
    run_project_local_semantic_candidate(prepared, request)
}

pub(super) fn run_ordinary_project_local_semantics<'ast>(
    product: &LibraryDeclarationProduct,
    library_units: Vec<LibraryBinderUnit>,
    bound: BoundProjectBinder,
    programs: &[ProjectProgram<'ast>],
) -> Result<Vec<CheckResult>, ProjectLocalCandidateErrorForTest> {
    let mut prepared =
        prepare_ordinary_project_local_execution(product, library_units, bound, programs)?;
    run_prepared_ordinary_project_local_semantics(&mut prepared)
}

fn run_prepared_ordinary_project_local_semantics(
    prepared: &mut PreparedProjectLocalExecution<'_, '_>,
) -> Result<Vec<CheckResult>, ProjectLocalCandidateErrorForTest> {
    let ordinary_validation_keys = prepared.ordinary_validation_keys.clone();
    let ordinary_materialized_keys = prepared.ordinary_materialized_keys.clone();
    let library_validations = prepared.coordinator.validations_for(
        ordinary_validation_keys.iter().copied(),
        ordinary_materialized_keys.iter().copied(),
    );
    let library_validation_groups = library_validation_groups(&prepared.coordinator);
    let output = run_ordinary_project_local_user_pass(
        prepared,
        library_validations,
        library_validation_groups,
    )?;
    finish_project_local_publication(prepared, &output)?;
    Ok(output.results)
}

#[cfg(feature = "session-bench")]
pub(super) struct SessionBenchSemanticReceipt {
    pub(super) results: Vec<CheckResult>,
    pub(super) materialized_product_owner_count: usize,
    pub(super) project_identity: u64,
    pub(super) publisher_identity: u64,
    pub(super) cache_identity: u64,
    pub(super) session_weak_alive_after: bool,
    pub(super) cache_weak_alive_after: bool,
}

#[cfg(feature = "session-bench")]
pub(super) fn run_ordinary_project_local_semantics_for_session_bench<'ast>(
    product: &LibraryDeclarationProduct,
    library_units: Vec<LibraryBinderUnit>,
    bound: BoundProjectBinder,
    programs: &[ProjectProgram<'ast>],
) -> Result<SessionBenchSemanticReceipt, ProjectLocalCandidateErrorForTest> {
    let mut prepared =
        prepare_ordinary_project_local_execution(product, library_units, bound, programs)?;
    let project_identity = prepared.session.project_identity();
    let publisher_identity = prepared.session.publisher_identity();
    let cache_identity = prepared.session.cache_identity();
    let session_weak = Rc::downgrade(&prepared.session);
    let cache_weak = prepared.session.cache_weak();
    let results = run_prepared_ordinary_project_local_semantics(&mut prepared)?;
    let materialized_product_owner_count = prepared.coordinator.materialized().len();
    drop(prepared);
    Ok(SessionBenchSemanticReceipt {
        results,
        materialized_product_owner_count,
        project_identity,
        publisher_identity,
        cache_identity,
        session_weak_alive_after: session_weak.upgrade().is_some(),
        cache_weak_alive_after: cache_weak.upgrade().is_some(),
    })
}

struct PreparedProjectLocalExecution<'run, 'ast> {
    product: &'run LibraryDeclarationProduct,
    #[cfg(any(test, feature = "test-utils"))]
    library_units: Vec<LibraryBinderUnit>,
    bound: Option<BoundProjectBinder>,
    programs: &'run [ProjectProgram<'ast>],
    session: Rc<SessionCore>,
    coordinator: ProjectLocalSemanticCoordinator,
    library_identity_groups: [Option<TypeGroupId>; 8],
    native_array_groups: [Option<usize>; 2],
    augmented_product_groups: std::collections::BTreeMap<usize, TypeGroupId>,
    user_product_dependencies: std::collections::BTreeMap<TypeGroupId, Vec<usize>>,
    #[cfg(any(test, feature = "test-utils"))]
    ordinary_demand_root_keys: Vec<usize>,
    ordinary_validation_keys: Vec<usize>,
    ordinary_materialized_keys: Vec<usize>,
}

struct ProjectLocalExecutionInputs<'run, 'ast> {
    product: &'run LibraryDeclarationProduct,
    #[cfg(any(test, feature = "test-utils"))]
    library_units: Vec<LibraryBinderUnit>,
    bound: BoundProjectBinder,
    programs: &'run [ProjectProgram<'ast>],
}

#[cfg(any(test, feature = "test-utils"))]
struct PreparedProjectLocalExecutionForTest<'run, 'ast> {
    prepared: PreparedProjectLocalExecution<'run, 'ast>,
    fault_state: FaultStateForTest,
}

fn prepare_ordinary_project_local_execution<'run, 'ast>(
    product: &'run LibraryDeclarationProduct,
    library_units: Vec<LibraryBinderUnit>,
    bound: BoundProjectBinder,
    programs: &'run [ProjectProgram<'ast>],
) -> Result<PreparedProjectLocalExecution<'run, 'ast>, ProjectLocalCandidateErrorForTest> {
    let session = SessionCore::start();
    let mapped_groups = mapped_product_groups(product, &bound)?;
    let coordinator =
        ProjectLocalSemanticCoordinator::new_ordinary(product, mapped_groups, Rc::clone(&session));
    let final_type_dependencies = final_type_dependencies(product, &bound, programs)?;
    #[cfg(not(any(test, feature = "test-utils")))]
    let _ = library_units;
    prepare_project_local_execution(
        ProjectLocalExecutionInputs {
            product,
            #[cfg(any(test, feature = "test-utils"))]
            library_units,
            bound,
            programs,
        },
        session,
        coordinator,
        final_type_dependencies,
        no_additional_product_demands,
        ProjectLocalSemanticCoordinator::commit_demanded_checked,
    )
}

#[cfg(any(test, feature = "test-utils"))]
fn prepare_project_local_execution_for_test<'run, 'ast>(
    product: &'run LibraryDeclarationProduct,
    library_units: Vec<LibraryBinderUnit>,
    bound: BoundProjectBinder,
    programs: &'run [ProjectProgram<'ast>],
    demand_roots: &[super::ProjectLocalDemandRootForTest],
    fault: Option<super::ProjectLocalSemanticFaultForTest>,
    force_all: bool,
) -> Result<PreparedProjectLocalExecutionForTest<'run, 'ast>, ProjectLocalCandidateErrorForTest> {
    let session = SessionCore::start();
    let mapped_groups = mapped_product_groups(product, &bound)?;
    let mut fault_state = FaultStateForTest::new(fault);
    let mut dependencies = ProjectLocalSemanticCoordinator::dependency_rows(product);
    if fault_state.activate(super::ProjectLocalSemanticFaultForTest::OmitDependencyEdge) {
        dependencies.pop();
    }
    let coordinator = ProjectLocalSemanticCoordinator::new_with_dependencies(
        product,
        mapped_groups,
        Rc::clone(&session),
        dependencies,
    );
    let requested_product_keys = demand_roots
        .iter()
        .filter_map(|root| match root {
            super::ProjectLocalDemandRootForTest::ProductGroup(group) => Some(group),
            _ => None,
        })
        .map(|group| {
            (coordinator.groups().get(group.product_key) == Some(group))
                .then_some(group.product_key)
                .ok_or(
                    ProjectLocalCandidateErrorForTest::MissingMappedProductGroup(group.product_key),
                )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let final_type_dependencies = if force_all {
        ProjectLocalFinalTypeDependencies {
            product: Vec::new(),
            user_products: std::collections::BTreeMap::new(),
        }
    } else {
        final_type_dependencies(product, &bound, programs)?
    };
    let prepared = prepare_project_local_execution(
        ProjectLocalExecutionInputs {
            product,
            library_units,
            bound,
            programs,
        },
        session,
        coordinator,
        final_type_dependencies,
        move |coordinator, product, augmented_product_groups, native_array_groups| {
            if requested_product_keys.is_empty() {
                return Ok(());
            }
            for root in &requested_product_keys {
                let _ = coordinator.demand_product_groups(
                    product,
                    &[*root],
                    augmented_product_groups,
                    native_array_groups,
                );
            }
            Ok(())
        },
        |coordinator| commit_demanded_for_test(coordinator, &mut fault_state),
    )?;
    Ok(PreparedProjectLocalExecutionForTest {
        prepared,
        fault_state,
    })
}

fn final_type_dependencies(
    product: &LibraryDeclarationProduct,
    bound: &BoundProjectBinder,
    programs: &[ProjectProgram<'_>],
) -> Result<ProjectLocalFinalTypeDependencies, ProjectLocalCandidateErrorForTest> {
    project_local_final_type_dependencies(product, bound, programs)
        .map_err(ProjectLocalCandidateErrorForTest::UserPass)
}

fn mapped_product_groups(
    product: &LibraryDeclarationProduct,
    bound: &BoundProjectBinder,
) -> Result<Vec<ProjectLocalMappedProductGroupForTest>, ProjectLocalCandidateErrorForTest> {
    let map = bound
        .library_product_map_for_test()
        .ok_or(ProjectLocalCandidateErrorForTest::MissingProductBinderMap)?;
    let product_groups = product
        .groups()
        .iter()
        .enumerate()
        .map(
            |(product_key, group)| super::ProjectLocalProductGroupForTest {
                product_key,
                container_key: group.container.index(),
                compilation_global: product.containers()[group.container.index()].kind
                    == ContainerKind::CompilationGlobal,
                name: product.strings()[group.name.index()].clone(),
                has_type: group.slots.r#type,
                has_value: group.slots.value,
                has_namespace: group.slots.namespace,
            },
        )
        .collect::<Vec<_>>();
    product_groups
        .iter()
        .map(|product_group| {
            let key = product.group_key_at(product_group.product_key).ok_or(
                ProjectLocalCandidateErrorForTest::MissingMappedProductGroup(
                    product_group.product_key,
                ),
            )?;
            let binding = map.group(key).ok_or(
                ProjectLocalCandidateErrorForTest::MissingMappedProductGroup(
                    product_group.product_key,
                ),
            )?;
            Ok(ProjectLocalMappedProductGroupForTest {
                product: product_group.clone(),
                local_symbol: binding.symbol().index(),
                local_type_group: binding.ty().map(|identity| identity.index()),
                local_value: binding.value().map(|identity| identity.index()),
                local_namespace: binding.namespace().map(|identity| identity.index()),
            })
        })
        .collect()
}

fn no_additional_product_demands(
    _coordinator: &mut ProjectLocalSemanticCoordinator,
    _product: &LibraryDeclarationProduct,
    _augmented_product_groups: &std::collections::BTreeMap<usize, TypeGroupId>,
    _native_array_groups: [Option<usize>; 2],
) -> Result<(), &'static str> {
    Ok(())
}

fn prepare_project_local_execution<'run, 'ast, Demand, Commit>(
    inputs: ProjectLocalExecutionInputs<'run, 'ast>,
    session: Rc<SessionCore>,
    mut coordinator: ProjectLocalSemanticCoordinator,
    final_type_dependencies: ProjectLocalFinalTypeDependencies,
    mut demand: Demand,
    mut commit: Commit,
) -> Result<PreparedProjectLocalExecution<'run, 'ast>, ProjectLocalCandidateErrorForTest>
where
    Demand: FnMut(
        &mut ProjectLocalSemanticCoordinator,
        &LibraryDeclarationProduct,
        &std::collections::BTreeMap<usize, TypeGroupId>,
        [Option<usize>; 2],
    ) -> Result<(), &'static str>,
    Commit: FnMut(&mut ProjectLocalSemanticCoordinator) -> Result<(), &'static str>,
{
    let ProjectLocalExecutionInputs {
        product,
        #[cfg(any(test, feature = "test-utils"))]
        library_units,
        bound,
        programs,
    } = inputs;
    let library_identity_groups = [
        "Array",
        "ReadonlyArray",
        "String",
        "Number",
        "Boolean",
        "RegExp",
        "Object",
        "CallableFunction",
    ]
    .map(|name| {
        coordinator
            .mapped_groups()
            .iter()
            .find(|row| row.product.compilation_global && row.product.name == name)
            .and_then(|row| row.local_type_group)
            .map(|index| {
                TypeGroupId(u32::try_from(index).expect("local type group identity fits u32"))
            })
    });
    let native_array_groups =
        [library_identity_groups[0], library_identity_groups[1]].map(|identity| {
            identity.and_then(|identity| {
                coordinator
                    .mapped_groups()
                    .iter()
                    .find(|row| row.local_type_group == Some(identity.index()))
                    .map(|row| row.product.product_key)
            })
        });
    let augmented_product_groups =
        project_local_user_augmented_product_groups(&bound, coordinator.mapped_groups());
    let augmented_roots = augmented_product_groups.keys().copied().collect::<Vec<_>>();
    for root in augmented_roots {
        let _ = coordinator.demand_product_groups(
            product,
            &[root],
            &augmented_product_groups,
            native_array_groups,
        );
    }
    demand(
        &mut coordinator,
        product,
        &augmented_product_groups,
        native_array_groups,
    )
    .map_err(ProjectLocalCandidateErrorForTest::UserPass)?;
    #[cfg(any(test, feature = "test-utils"))]
    let ordinary_demand_root_keys = coordinator.demand_roots().to_vec();
    let ordinary_validation_closure = coordinator.validation_owner_closure(
        augmented_product_groups.keys().copied(),
        &final_type_dependencies.product,
    );
    let _ = coordinator.demand_product_groups(
        product,
        &ordinary_validation_closure,
        &augmented_product_groups,
        native_array_groups,
    );
    commit(&mut coordinator).map_err(ProjectLocalCandidateErrorForTest::UserPass)?;
    let ordinary_materialized_keys = coordinator.materialized();
    let ordinary_validation_keys = ordinary_validation_closure
        .into_iter()
        .filter(|key| coordinator.ready().contains(key))
        .collect::<Vec<_>>();
    Ok(PreparedProjectLocalExecution {
        product,
        #[cfg(any(test, feature = "test-utils"))]
        library_units,
        bound: Some(bound),
        programs,
        session,
        coordinator,
        library_identity_groups,
        native_array_groups,
        augmented_product_groups,
        user_product_dependencies: final_type_dependencies.user_products,
        #[cfg(any(test, feature = "test-utils"))]
        ordinary_demand_root_keys,
        ordinary_validation_keys,
        ordinary_materialized_keys,
    })
}

#[cfg(any(test, feature = "test-utils"))]
fn commit_demanded_for_test(
    coordinator: &mut ProjectLocalSemanticCoordinator,
    fault_state: &mut FaultStateForTest,
) -> Result<(), &'static str> {
    let (injection, reached) = if fault_state.is_available() {
        match fault_state.requested() {
            Some(super::ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete) => (
                PublicationInjection::PublishBeforeSccComplete,
                Some(super::ProjectLocalSemanticFaultForTest::PublishBeforeSccComplete),
            ),
            Some(super::ProjectLocalSemanticFaultForTest::WriteResolvingToDurableCache) => (
                PublicationInjection::WriteResolvingToDurableCache,
                Some(super::ProjectLocalSemanticFaultForTest::WriteResolvingToDurableCache),
            ),
            _ => (PublicationInjection::None, None),
        }
    } else {
        (PublicationInjection::None, None)
    };
    coordinator.commit_demanded_checked_with_injection(injection, |activated| {
        debug_assert_eq!(activated, injection);
        fault_state
            .record_publication(reached.expect("publication injection has an authenticated fault"));
    })
}

fn library_validation_groups(
    coordinator: &ProjectLocalSemanticCoordinator,
) -> std::collections::BTreeMap<usize, TypeGroupId> {
    coordinator
        .mapped_groups()
        .iter()
        .filter_map(|row| {
            row.local_type_group.map(|local| {
                (
                    row.product.product_key,
                    TypeGroupId(u32::try_from(local).expect("local type group identity fits u32")),
                )
            })
        })
        .collect()
}

fn run_ordinary_project_local_user_pass(
    prepared: &mut PreparedProjectLocalExecution<'_, '_>,
    library_validations: Vec<super::validation::ProjectLocalValidationCandidate>,
    library_validation_groups: std::collections::BTreeMap<usize, TypeGroupId>,
) -> Result<ProjectLocalUserPassOutput, ProjectLocalCandidateErrorForTest> {
    let bound = prepared
        .bound
        .take()
        .expect("prepared project-local execution owns its binder");
    run_project_local_user_pass_core(
        prepared.product,
        prepared.programs,
        &prepared.session,
        &mut prepared.coordinator,
        &prepared.augmented_product_groups,
        &prepared.user_product_dependencies,
        prepared.native_array_groups,
        bound,
        prepared.library_identity_groups,
        library_validations,
        library_validation_groups,
        Vec::new(),
        ProjectLocalSemanticCoordinator::commit_demanded_checked,
        run_project_local_user_pass,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_project_local_user_pass_core<'ast, Commit, Run>(
    product: &LibraryDeclarationProduct,
    programs: &[ProjectProgram<'ast>],
    session: &Rc<SessionCore>,
    coordinator: &mut ProjectLocalSemanticCoordinator,
    augmented_product_groups: &std::collections::BTreeMap<usize, TypeGroupId>,
    user_product_dependencies: &std::collections::BTreeMap<TypeGroupId, Vec<usize>>,
    native_array_groups: [Option<usize>; 2],
    bound: BoundProjectBinder,
    library_identity_groups: [Option<TypeGroupId>; 8],
    library_validations: Vec<super::validation::ProjectLocalValidationCandidate>,
    library_validation_groups: std::collections::BTreeMap<usize, TypeGroupId>,
    user_namespace_demands: Vec<String>,
    commit: Commit,
    run: Run,
) -> Result<ProjectLocalUserPassOutput, ProjectLocalCandidateErrorForTest>
where
    Commit: FnMut(&mut ProjectLocalSemanticCoordinator) -> Result<(), &'static str>,
    Run: for<'pass> FnOnce(
        &'pass mut super::project_local_pass_port::Interner,
        ProjectLocalUserPassRequest<'pass, 'ast>,
    ) -> Result<ProjectLocalUserPassOutput, &'static str>,
{
    let published_types = project_local_published_types(
        product,
        coordinator.mapped_groups(),
        ProjectLocalPublishedTypeInputs {
            group_types: coordinator.semantic_group_types(),
            type_parameter_ids: coordinator.type_parameter_ids(),
            type_parameter_defaults: coordinator.type_parameter_defaults(),
            ready_product_groups: coordinator.ready(),
            unavailable_product_groups: coordinator.unavailable(),
            aliases: coordinator.aliases().clone(),
        },
    );
    let lexical_array_alias = coordinator
        .mapped_groups()
        .iter()
        .find(|row| row.product.name == "Array")
        .and_then(|row| row.local_type_group)
        .map(|index| {
            TypeGroupId(u32::try_from(index).expect("local type group identity fits u32"))
        });
    let next_type_param = u32::try_from(coordinator.type_parameter_ids().len())
        .expect("product type parameter count fits u32");
    let product_value_types = coordinator
        .mapped_groups()
        .iter()
        .filter_map(|row| {
            let storage = row.local_value?;
            let ty = coordinator
                .semantic_group_value_types()
                .get(row.product.product_key)
                .copied()
                .flatten()?;
            Some((
                super::project_local_pass_port::ValueStorageId(
                    u32::try_from(storage).expect("local value identity fits u32"),
                ),
                ty,
            ))
        })
        .collect();
    let product_namespace_seeds = coordinator
        .mapped_groups()
        .iter()
        .filter_map(|row| {
            let namespace = row.local_namespace?;
            let storage = row.local_value?;
            let ty = coordinator
                .semantic_group_value_types()
                .get(row.product.product_key)
                .copied()
                .flatten()?;
            Some(ProjectLocalNamespaceSeed {
                namespace: NamespaceId(
                    u32::try_from(namespace).expect("local namespace identity fits u32"),
                ),
                storage: super::project_local_pass_port::ValueStorageId(
                    u32::try_from(storage).expect("local value identity fits u32"),
                ),
                ty,
            })
        })
        .collect();
    let product_type_overlays = coordinator.overlays().to_vec();
    session
        .with_interner(|interner| {
            let mut demand_gateway = ProjectLocalPassDemandGateway::with_commit(
                coordinator,
                product,
                augmented_product_groups,
                native_array_groups,
                user_product_dependencies,
                commit,
            );
            run(
                interner,
                ProjectLocalUserPassRequest {
                    bound,
                    programs,
                    published_types,
                    product_value_types,
                    product_namespace_seeds,
                    product_type_overlays,
                    user_namespace_demands,
                    library_identity_groups,
                    next_type_param,
                    lexical_array_alias,
                    demand_gateway: &mut demand_gateway,
                    library_validations,
                    library_validation_groups,
                },
            )
        })
        .map_err(ProjectLocalCandidateErrorForTest::UserPass)
}

fn finish_project_local_publication(
    prepared: &mut PreparedProjectLocalExecution<'_, '_>,
    output: &ProjectLocalUserPassOutput,
) -> Result<(), ProjectLocalCandidateErrorForTest> {
    finish_project_local_publication_parts(&mut prepared.coordinator, output)
}

fn finish_project_local_publication_parts(
    coordinator: &mut ProjectLocalSemanticCoordinator,
    output: &ProjectLocalUserPassOutput,
) -> Result<(), ProjectLocalCandidateErrorForTest> {
    finish_project_local_publication_parts_inner(
        coordinator,
        output,
        ProjectLocalSemanticCoordinator::commit_demanded_checked,
    )
}

#[cfg(any(test, feature = "test-utils"))]
fn finish_project_local_publication_parts_for_test(
    coordinator: &mut ProjectLocalSemanticCoordinator,
    output: &ProjectLocalUserPassOutput,
    fault_state: &mut FaultStateForTest,
) -> Result<(), ProjectLocalCandidateErrorForTest> {
    finish_project_local_publication_parts_inner(coordinator, output, |coordinator| {
        commit_demanded_for_test(coordinator, fault_state)
    })
}

fn finish_project_local_publication_parts_inner<Commit>(
    coordinator: &mut ProjectLocalSemanticCoordinator,
    output: &ProjectLocalUserPassOutput,
    mut commit: Commit,
) -> Result<(), ProjectLocalCandidateErrorForTest>
where
    Commit: FnMut(&mut ProjectLocalSemanticCoordinator) -> Result<(), &'static str>,
{
    for (local, overlay) in coordinator.overlays() {
        let stable = matches!(
            output.published_type_groups.get(local.index()),
            Some(super::project_local_pass_port::PublishedTypeGroupTerminal::Ready(group))
                if group.surface
                    == super::project_local_pass_port::PublishedTypeGroupSurface::Template(
                        overlay.template
                    )
        );
        if !stable {
            return Err(ProjectLocalCandidateErrorForTest::UserPass(
                "project-local type overlay changed its reserved semantic identity",
            ));
        }
    }
    commit(coordinator).map_err(ProjectLocalCandidateErrorForTest::UserPass)
}

#[cfg(any(test, feature = "test-utils"))]
fn run_project_local_semantic_candidate(
    prepared_for_test: PreparedProjectLocalExecutionForTest<'_, '_>,
    request: ProjectLocalSemanticRequestForTest,
) -> Result<ProjectLocalSemanticTraceForTest, ProjectLocalCandidateErrorForTest> {
    let PreparedProjectLocalExecutionForTest {
        prepared,
        mut fault_state,
    } = prepared_for_test;
    let PreparedProjectLocalExecution {
        product,
        library_units,
        mut bound,
        programs,
        session,
        mut coordinator,
        library_identity_groups,
        native_array_groups,
        augmented_product_groups,
        user_product_dependencies,
        ordinary_demand_root_keys,
        ordinary_validation_keys,
        ordinary_materialized_keys,
    } = prepared;
    let project_session_identity = session.project_identity();
    let publisher_session_identity = session.publisher_identity();
    let bound = bound
        .take()
        .expect("prepared project-local execution owns its binder");
    let trace_projection_inputs = (
        project_local_final_type_group_fragment_counts(&bound, coordinator.mapped_groups()),
        project_local_final_value_declaration_counts(&bound, coordinator.mapped_groups()),
        project_local_probe_authority(product, &bound)
            .ok_or(ProjectLocalCandidateErrorForTest::MissingProductBinderMap)?,
    );
    let ordinary_incomplete_records = coordinator.library_records();
    let (force_all_visited_keys, force_all_additional_materialized_keys) = if request.force_all {
        let all_groups = coordinator
            .groups()
            .iter()
            .map(|group| group.product_key)
            .collect::<Vec<_>>();
        let ordinary_materialized = ordinary_materialized_keys
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let _ = coordinator.demand_product_groups_for_phase(
            product,
            &all_groups,
            &augmented_product_groups,
            native_array_groups,
            ProjectLocalSemanticPhaseForTest::ForceAll,
        );
        commit_demanded_for_test(&mut coordinator, &mut fault_state)
            .map_err(ProjectLocalCandidateErrorForTest::UserPass)?;
        let additional = coordinator
            .materialized()
            .into_iter()
            .filter(|key| !ordinary_materialized.contains(key))
            .collect::<Vec<_>>();
        (all_groups, additional)
    } else {
        (Vec::new(), Vec::new())
    };
    let force_all_validation_keys = request.force_all.then(|| coordinator.ready_groups());
    let force_all_incomplete_records = request.force_all.then(|| coordinator.library_records());
    let scheduled_validation_keys = if request.force_all {
        force_all_validation_keys
            .as_ref()
            .expect("force_all validation roots were captured")
    } else {
        &ordinary_validation_keys
    };
    let visited_validation_keys = if request.force_all {
        &force_all_visited_keys
    } else {
        &ordinary_materialized_keys
    };
    let library_validations = coordinator.validations_for(
        scheduled_validation_keys.iter().copied(),
        visited_validation_keys.iter().copied(),
    );
    let library_validation_groups = library_validation_groups(&coordinator);
    let global_publication =
        publish_final_global_contributors(product, &bound, programs, request.fault)
            .map_err(ProjectLocalCandidateErrorForTest::GlobalObject)?;
    let user_namespace_demands = request
        .demand_roots
        .iter()
        .filter_map(|root| match root {
            super::ProjectLocalDemandRootForTest::UserNamespace(name) => Some(name.clone()),
            _ => None,
        })
        .collect();
    let raw_user_output = run_project_local_user_pass_core(
        product,
        programs,
        &session,
        &mut coordinator,
        &augmented_product_groups,
        &user_product_dependencies,
        native_array_groups,
        bound,
        library_identity_groups,
        library_validations,
        library_validation_groups,
        user_namespace_demands,
        |coordinator| commit_demanded_for_test(coordinator, &mut fault_state),
        |interner, pass_request| {
            run_project_local_user_pass_for_test(interner, pass_request, request.fault)
        },
    )?;
    finish_project_local_publication_parts_for_test(
        &mut coordinator,
        &raw_user_output,
        &mut fault_state,
    )?;
    let (final_type_group_fragment_counts, final_value_declaration_counts, probe_authority) =
        trace_projection_inputs;
    let library_source_paths = product
        .sources()
        .iter()
        .map(|source| {
            (
                super::project_local_pass_port::LibraryFileOrdinal::new(
                    usize::try_from(source.ordinal).expect("library source ordinal fits usize"),
                ),
                product.strings()[source.name.index()].clone(),
            )
        })
        .collect();
    let mut source_programs = programs.iter().collect::<Vec<_>>();
    source_programs.sort_by_key(|program| match program.compilation_unit.origin {
        super::project_local_pass_port::CompilationOrigin::User(ordinal) => ordinal.index(),
        super::project_local_pass_port::CompilationOrigin::Library(ordinal) => ordinal.index(),
    });
    let source_inputs = source_programs
        .iter()
        .map(|program| ProjectLocalSourceInputForTest {
            path: program.normalized_path.clone(),
            byte_len: program.program.source_text.len(),
            sha256: sha256_hex(program.program.source_text.as_bytes()),
        })
        .collect::<Vec<_>>();
    let source_visit_rows = source_inputs
        .iter()
        .map(|input| input.path.clone())
        .collect::<Vec<_>>();
    let user_output =
        project_local_user_pass_observables(&raw_user_output, programs, &library_source_paths)
            .map_err(ProjectLocalCandidateErrorForTest::UserPass)?;
    let materialized_keys = ordinary_materialized_keys;
    let materialized_product_groups = materialized_keys
        .iter()
        .map(|key| coordinator.groups()[*key].clone())
        .collect::<Vec<_>>();
    let mut demand_root_visits = ordinary_demand_root_keys
        .iter()
        .map(|key| {
            super::ProjectLocalDemandRootForTest::ProductGroup(coordinator.groups()[*key].clone())
        })
        .collect::<Vec<_>>();
    demand_root_visits.extend(
        request
            .demand_roots
            .iter()
            .filter(|root| !matches!(root, super::ProjectLocalDemandRootForTest::ProductGroup(_)))
            .cloned(),
    );
    let semantic_probes = session.with_interner(|interner| {
        let published = project_local_published_types(
            product,
            coordinator.mapped_groups(),
            ProjectLocalPublishedTypeInputs {
                group_types: coordinator.semantic_group_types(),
                type_parameter_ids: coordinator.type_parameter_ids(),
                type_parameter_defaults: coordinator.type_parameter_defaults(),
                ready_product_groups: coordinator.ready(),
                unavailable_product_groups: coordinator.unavailable(),
                aliases: coordinator.aliases().clone(),
            },
        );
        let query_state =
            project_local_semantic_query_state(library_identity_groups, &published, interner);
        build_semantic_probes(
            interner,
            query_state,
            SemanticProbeInput {
                materialized: &materialized_keys,
                group_types: coordinator.semantic_group_types(),
                group_value_types: coordinator.semantic_group_value_types(),
                final_type_group_fragment_counts: &final_type_group_fragment_counts,
                final_value_declaration_counts: &final_value_declaration_counts,
                authority: &probe_authority,
                include_namespace_values: false,
            },
        )
    });
    let repeated_roots = coordinator
        .demand_roots()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let first_demand_identities = repeated_roots
        .iter()
        .filter_map(|key| coordinator.repeat(*key).map(|identity| (*key, identity)))
        .collect::<BTreeSet<_>>();
    let _ = coordinator.demand_product_groups(
        product,
        &repeated_roots,
        &augmented_product_groups,
        native_array_groups,
    );
    let repeated_demands = first_demand_identities
        .into_iter()
        .filter_map(|(key, first_identity)| {
            coordinator
                .repeat(key)
                .map(|repeated_identity| ProjectLocalRepeatedDemandForTest {
                    owner: coordinator.groups()[key].clone(),
                    first_identity,
                    repeated_identity,
                })
        })
        .collect::<Vec<_>>();
    let ordinary_validation_set = ordinary_validation_keys
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let mut ordinary_production_records = ordinary_incomplete_records;
    ordinary_production_records.extend(
        user_output
            .library_records
            .iter()
            .filter(|(group, _)| ordinary_validation_set.contains(group))
            .map(|(_, record)| record.clone()),
    );
    ordinary_production_records.sort();
    let force_all_production_records = force_all_incomplete_records.map(|mut records| {
        records.extend(
            user_output
                .library_records
                .iter()
                .map(|(_, record)| record.clone()),
        );
        records.sort();
        records
    });
    let force_all = if request.force_all {
        let keys = force_all_visited_keys.clone();
        let requested_groups = keys
            .iter()
            .map(|key| coordinator.groups()[*key].clone())
            .collect::<Vec<_>>();
        let additional_materialized_product_groups = force_all_additional_materialized_keys
            .iter()
            .map(|key| coordinator.groups()[*key].clone())
            .collect::<Vec<_>>();
        let (semantic_probes, terminal_census) = session
            .with_interner(|interner| {
                let published = project_local_published_types(
                    product,
                    coordinator.mapped_groups(),
                    ProjectLocalPublishedTypeInputs {
                        group_types: coordinator.semantic_group_types(),
                        type_parameter_ids: coordinator.type_parameter_ids(),
                        type_parameter_defaults: coordinator.type_parameter_defaults(),
                        ready_product_groups: coordinator.ready(),
                        unavailable_product_groups: coordinator.unavailable(),
                        aliases: coordinator.aliases().clone(),
                    },
                );
                let query_state = project_local_semantic_query_state(
                    library_identity_groups,
                    &published,
                    interner,
                );
                let probes = build_semantic_probes(
                    interner,
                    query_state,
                    SemanticProbeInput {
                        materialized: &keys,
                        group_types: coordinator.semantic_group_types(),
                        group_value_types: coordinator.semantic_group_value_types(),
                        final_type_group_fragment_counts: &final_type_group_fragment_counts,
                        final_value_declaration_counts: &final_value_declaration_counts,
                        authority: &probe_authority,
                        include_namespace_values: true,
                    },
                );
                coordinator
                    .force_all_terminal_census(interner)
                    .map(|terminals| (probes, terminals))
            })
            .map_err(ProjectLocalCandidateErrorForTest::UserPass)?;
        Some(super::ProjectLocalForceAllTraceForTest {
            requested_product_groups: requested_groups,
            additional_materialized_owner_work_count: additional_materialized_product_groups.len(),
            additional_materialized_product_groups,
            production_validation_roots: force_all_validation_keys
                .as_ref()
                .expect("force_all validation roots were captured")
                .iter()
                .map(|key| coordinator.groups()[*key].clone())
                .collect(),
            terminal_census,
            semantic_probes,
            library_records: force_all_production_records
                .clone()
                .expect("force_all production records were captured"),
        })
    } else {
        None
    };
    let scc_publications = user_output
        .unavailable_audits
        .iter()
        .map(|audit| {
            let members = vec![audit.terminal.name.clone()];
            ProjectLocalSccPublicationForTest {
                scc: ProjectLocalSccForTest {
                    members: members.clone(),
                },
                reserved_members: members,
                published_members: Vec::new(),
                observed_before_complete: Vec::new(),
                state_before_publish: ProjectLocalTerminalStateForTest::Resolving,
                final_state: ProjectLocalTerminalStateForTest::Unavailable(audit.terminal.reason),
                discarded_provisional_writes: audit.discarded_provisional_writes.clone(),
            }
        })
        .collect();
    let ordinary_force_all_invocations = if fault_state
        .activate(super::ProjectLocalSemanticFaultForTest::InvokeForceAllFromOrdinaryDemand)
    {
        Vec::from(["fault".to_owned()])
    } else {
        Vec::new()
    };
    let check_results = raw_user_output.results;
    let trace = ProjectLocalSemanticTraceForTest {
        case_id: request.case_id.to_owned(),
        project_session_identity,
        publisher_session_identity,
        source_inputs,
        source_visit_rows,
        admitted_product_groups: coordinator.groups().to_vec(),
        mapped_product_groups: coordinator.mapped_groups().to_vec(),
        product_dependencies: coordinator.dependencies().to_vec(),
        product_publication_sccs: coordinator.sccs().to_vec(),
        demand_root_visits,
        materialized_owner_work_count: materialized_product_groups.len(),
        materialized_product_groups,
        production_validation_roots: ordinary_validation_keys
            .iter()
            .map(|key| coordinator.groups()[*key].clone())
            .collect(),
        production_library_records: ordinary_production_records,
        semantic_probes,
        user_event_ledger: user_output.event_ledger,
        user_check_results: user_output.check_results,
        unavailable_audits: user_output.unavailable_audits,
        scc_publications,
        publication_scc_commits: coordinator.commits().to_vec(),
        durable_cache_writes: coordinator.cache_writes().to_vec(),
        published_global_contributors: global_publication.published_contributors,
        final_global_contributors: global_publication.final_contributors,
        repeated_demands,
        ordinary_force_all_invocations,
        force_all,
    };
    session.receipt_built();
    drop((coordinator, library_units));
    drop(session);
    drop(check_results);
    Ok(trace)
}

//! Capability-limited entry for the project-local semantic candidate.

use super::coordinator::{ProjectLocalPassDemandGateway, ProjectLocalSemanticCoordinator};
use super::global_owner::publish_final_global_contributors;
use super::lowerer::{build_semantic_probes, SemanticProbeInput};
use super::project_local_pass_port::{
    project_local_final_product_dependencies, project_local_final_type_group_fragment_counts,
    project_local_final_value_declaration_counts, project_local_probe_authority,
    project_local_published_types, project_local_semantic_query_state,
    project_local_user_augmented_product_groups, run_project_local_user_pass, sha256_hex,
    BoundProjectBinder, ContainerKind, LibraryBinderUnit, LibraryDeclarationProduct, NamespaceId,
    ProjectLocalNamespaceSeed, ProjectLocalUserPassRequest, ProjectProgram, TypeGroupId,
};
use super::session::SessionCore;
use super::{
    ProjectLocalMappedProductGroupForTest, ProjectLocalRepeatedDemandForTest,
    ProjectLocalSccForTest, ProjectLocalSccPublicationForTest, ProjectLocalSemanticPhaseForTest,
    ProjectLocalSemanticRequestForTest, ProjectLocalSemanticTraceForTest,
    ProjectLocalSourceInputForTest, ProjectLocalTerminalStateForTest,
};
use std::collections::BTreeSet;
use std::fmt;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectLocalCandidateErrorForTest {
    NotImplemented,
    MissingProductBinderMap,
    MissingMappedProductGroup(usize),
    GlobalObject(&'static str),
    UserPass(&'static str),
}

impl fmt::Display for ProjectLocalCandidateErrorForTest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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
            Self::GlobalObject(message) => formatter.write_str(message),
            Self::UserPass(message) => formatter.write_str(message),
        }
    }
}

pub fn project_local_semantic_candidate_for_test<'ast>(
    product: &LibraryDeclarationProduct,
    library_units: Vec<LibraryBinderUnit>,
    bound: BoundProjectBinder,
    programs: &[ProjectProgram<'ast>],
    request: ProjectLocalSemanticRequestForTest,
) -> Result<ProjectLocalSemanticTraceForTest, ProjectLocalCandidateErrorForTest> {
    let session = SessionCore::start();
    let project_session_identity = session.project_identity();
    let publisher_session_identity = session.publisher_identity();
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
    let mapped_groups = product_groups
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
        .collect::<Result<Vec<_>, _>>()?;
    let mut coordinator = ProjectLocalSemanticCoordinator::new(
        product,
        mapped_groups,
        Rc::clone(&session),
        request.fault,
    );
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
    let final_product_dependencies = if request.force_all || augmented_product_groups.is_empty() {
        Vec::new()
    } else {
        project_local_final_product_dependencies(product, &bound, programs)
            .map_err(ProjectLocalCandidateErrorForTest::UserPass)?
    };
    let final_type_group_fragment_counts =
        project_local_final_type_group_fragment_counts(&bound, coordinator.mapped_groups());
    let final_value_declaration_counts =
        project_local_final_value_declaration_counts(&bound, coordinator.mapped_groups());
    let probe_authority = project_local_probe_authority(product, &bound)
        .ok_or(ProjectLocalCandidateErrorForTest::MissingProductBinderMap)?;
    let augmented_roots = augmented_product_groups.keys().copied().collect::<Vec<_>>();
    for root in augmented_roots {
        let _ = coordinator.demand_product_groups(
            product,
            &[root],
            &augmented_product_groups,
            native_array_groups,
        );
    }
    let ordinary_demand_root_keys = coordinator.demand_roots().to_vec();
    let ordinary_validation_closure = coordinator.validation_owner_closure(
        augmented_product_groups.keys().copied(),
        &final_product_dependencies,
    );
    let _ = coordinator.demand_product_groups(
        product,
        &ordinary_validation_closure,
        &augmented_product_groups,
        native_array_groups,
    );
    coordinator
        .commit_demanded_checked()
        .map_err(ProjectLocalCandidateErrorForTest::UserPass)?;
    let ordinary_materialized_keys = coordinator.materialized();
    let ordinary_validation_keys = ordinary_validation_closure
        .into_iter()
        .filter(|key| coordinator.ready().contains(key))
        .collect::<Vec<_>>();
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
        coordinator
            .commit_demanded_checked()
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
    let library_validation_groups = coordinator
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
        .collect();
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
    let global_publication =
        publish_final_global_contributors(product, &bound, programs, request.fault)
            .map_err(ProjectLocalCandidateErrorForTest::GlobalObject)?;
    let published_types = project_local_published_types(
        product,
        coordinator.mapped_groups(),
        coordinator.semantic_group_types(),
        coordinator.type_parameter_ids(),
        coordinator.type_parameter_defaults(),
        coordinator.ready(),
        coordinator.unavailable(),
        coordinator.aliases().clone(),
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
    let user_namespace_demands = request
        .demand_roots
        .iter()
        .filter_map(|root| match root {
            super::ProjectLocalDemandRootForTest::UserNamespace(name) => Some(name.clone()),
            _ => None,
        })
        .collect();
    let product_type_overlays = coordinator.overlays().to_vec();
    let user_output = session
        .with_interner(|interner| {
            let mut demand_gateway = ProjectLocalPassDemandGateway::new(
                &mut coordinator,
                product,
                &augmented_product_groups,
                native_array_groups,
            );
            run_project_local_user_pass(
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
                    library_source_paths,
                    fault: request.fault,
                },
            )
        })
        .map_err(ProjectLocalCandidateErrorForTest::UserPass)?;
    for (local, overlay) in coordinator.overlays() {
        let stable = matches!(
            user_output.published_type_groups.get(local.index()),
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
    coordinator
        .commit_demanded_checked()
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
            coordinator.semantic_group_types(),
            coordinator.type_parameter_ids(),
            coordinator.type_parameter_defaults(),
            coordinator.ready(),
            coordinator.unavailable(),
            coordinator.aliases().clone(),
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
                    coordinator.semantic_group_types(),
                    coordinator.type_parameter_ids(),
                    coordinator.type_parameter_defaults(),
                    coordinator.ready(),
                    coordinator.unavailable(),
                    coordinator.aliases().clone(),
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
    let ordinary_force_all_invocations = if coordinator.invoke_force_all_from_ordinary_if_faulted()
    {
        Vec::from(["fault".to_owned()])
    } else {
        Vec::new()
    };
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
    Ok(trace)
}

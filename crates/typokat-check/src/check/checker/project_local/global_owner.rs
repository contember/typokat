//! Final global-object contributor publication.

use super::project_local_pass_port::{
    collect_final_global_object_contributors, source_global_binding_census_with_provenance,
    BoundProjectBinder, CompilationOrigin, LibraryDeclarationProduct, ProjectProgram,
    SourceBindingSlot, SourceGlobalContributorKind,
};
use super::{
    ProjectLocalContributorKindForTest, ProjectLocalContributorOriginForTest,
    ProjectLocalGlobalContributorForTest, ProjectLocalGlobalContributorSiteForTest,
    ProjectLocalSemanticFaultForTest,
};

pub(super) struct ProjectLocalGlobalPublication {
    pub(super) final_contributors: Vec<ProjectLocalGlobalContributorForTest>,
    pub(super) published_contributors: Vec<ProjectLocalGlobalContributorForTest>,
}

pub(super) fn publish_final_global_contributors(
    product: &LibraryDeclarationProduct,
    bound: &BoundProjectBinder,
    programs: &[ProjectProgram<'_>],
    fault: Option<ProjectLocalSemanticFaultForTest>,
) -> Result<ProjectLocalGlobalPublication, &'static str> {
    let final_contributors = final_global_contributors(product, bound, programs)?;
    let published_contributors = if super::fault_gate::activate_if(
        fault,
        ProjectLocalSemanticFaultForTest::PublishStaleGlobalObject,
    ) {
        Vec::new()
    } else {
        final_contributors.clone()
    };
    Ok(ProjectLocalGlobalPublication {
        final_contributors,
        published_contributors,
    })
}

fn final_global_contributors(
    product: &LibraryDeclarationProduct,
    bound: &BoundProjectBinder,
    programs: &[ProjectProgram<'_>],
) -> Result<Vec<ProjectLocalGlobalContributorForTest>, &'static str> {
    let inventory = collect_final_global_object_contributors(&bound.binder)
        .map_err(|_| "final global contributor inventory is unavailable")?;
    let mut rows = inventory
        .rows
        .into_iter()
        .map(|row| {
            let mut sites = row
                .sites
                .into_iter()
                .map(|site| {
                    let (source_path, origin) = match site.origin {
                        CompilationOrigin::Library(ordinal) => {
                            let source = product
                                .sources()
                                .get(ordinal.index())
                                .ok_or("library contributor source is unavailable")?;
                            (
                                product.strings()[source.name.index()].clone(),
                                ProjectLocalContributorOriginForTest::Library,
                            )
                        }
                        CompilationOrigin::User(ordinal) => {
                            let program = programs
                                .iter()
                                .find(|program| {
                                    program.compilation_unit.origin
                                        == CompilationOrigin::User(ordinal)
                                })
                                .ok_or("user contributor source is unavailable")?;
                            (
                                program.normalized_path.clone(),
                                ProjectLocalContributorOriginForTest::User,
                            )
                        }
                    };
                    let kind = match site.kind {
                        SourceGlobalContributorKind::Ordinary => {
                            ProjectLocalContributorKindForTest::Ordinary
                        }
                        SourceGlobalContributorKind::Namespace => {
                            ProjectLocalContributorKindForTest::Namespace
                        }
                    };
                    Ok(ProjectLocalGlobalContributorSiteForTest {
                        source_path,
                        origin,
                        span_start: site.span.start,
                        span_end: site.span.end,
                        kind,
                    })
                })
                .collect::<Result<Vec<_>, &'static str>>()?;
            sites.sort();
            Ok(ProjectLocalGlobalContributorForTest {
                name: row.name,
                has_value: row.value.is_some(),
                has_type: row.ty.is_some(),
                has_namespace: row.namespace.is_some(),
                sites,
            })
        })
        .collect::<Result<Vec<_>, &'static str>>()?;
    for program in programs {
        let provenance = source_global_binding_census_with_provenance(
            program.program,
            program.compilation_unit.binding,
        );
        for site in provenance.contributor_sites {
            let candidate = provenance
                .census
                .candidates
                .get(&site.name)
                .ok_or("user global contributor has no binding candidate")?;
            let normalized_site = ProjectLocalGlobalContributorSiteForTest {
                source_path: program.normalized_path.clone(),
                origin: ProjectLocalContributorOriginForTest::User,
                span_start: site.span.start,
                span_end: site.span.end,
                kind: match site.kind {
                    SourceGlobalContributorKind::Ordinary => {
                        ProjectLocalContributorKindForTest::Ordinary
                    }
                    SourceGlobalContributorKind::Namespace => {
                        ProjectLocalContributorKindForTest::Namespace
                    }
                },
            };
            if let Some(row) = rows.iter_mut().find(|row| row.name == site.name) {
                row.has_value |= candidate.slots.contains(&SourceBindingSlot::Value);
                row.has_type |= candidate.slots.contains(&SourceBindingSlot::Type);
                row.has_namespace |= candidate.slots.contains(&SourceBindingSlot::Namespace);
                if !row.sites.contains(&normalized_site) {
                    row.sites.push(normalized_site);
                    row.sites.sort();
                }
            } else {
                rows.push(ProjectLocalGlobalContributorForTest {
                    name: site.name,
                    has_value: candidate.slots.contains(&SourceBindingSlot::Value),
                    has_type: candidate.slots.contains(&SourceBindingSlot::Type),
                    has_namespace: candidate.slots.contains(&SourceBindingSlot::Namespace),
                    sites: vec![normalized_site],
                });
            }
        }
    }
    rows.sort();
    Ok(rows)
}

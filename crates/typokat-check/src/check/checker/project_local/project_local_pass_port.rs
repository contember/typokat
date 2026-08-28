//! Neutral observable output types for the project-local checker pass.

use super::validation::ProjectLocalValidationCandidate;
use super::{
    ProjectLocalCheckResultForTest, ProjectLocalContributorOriginForTest,
    ProjectLocalMappedProductGroupForTest, ProjectLocalNamespaceFragmentForTest,
    ProjectLocalSemanticFaultForTest, ProjectLocalUnavailableAuditForTest,
    ProjectLocalUnavailableReasonForTest, ProjectLocalUnavailableTerminalForTest,
    ProjectLocalUserEventRecordForTest,
};
pub(super) use crate::binder::bind::LibraryBinderUnit;
use crate::binder::declaration::SourceDeclarationManifest;
pub(super) use crate::binder::declaration::SourceGlobalContributorKind;
pub(super) use crate::binder::declaration::TypeGroupId;
pub(super) use crate::binder::declaration::ValueStorageId;
pub(super) use crate::binder::declaration::{
    source_global_binding_census_with_provenance, SourceBindingSlot,
};
pub(super) use crate::binder::namespace::NamespaceId;
pub(super) use crate::binder::roots::collect_final_global_object_contributors;
use crate::binder::scope::ScopeId;
use crate::binder::symbol::SymbolId;
use crate::check::checker::annotations::project_local_well_known_symbol_from_name;
pub(super) use crate::check::checker::context::{
    ProjectLocalDemandSnapshot, ProjectLocalTypeOverlayDraft, ProjectLocalUserDemandGateway,
};
pub(super) use crate::check::checker::eval::keyof_of_type;
use crate::check::checker::library_identities::LibrarySemanticIdentities;
pub(super) use crate::check::checker::namespace_values::ProjectLocalNamespaceSeed;
use crate::check::checker::reporting_record::CheckerRecord;
pub(super) use crate::check::checker::type_groups::PublishedTypeParameterDefault;
use crate::check::checker::type_groups::{
    PublishedTypeEnvironment, PublishedTypeGroup, PublishedTypeGroupUnavailable,
    TypeGroupUnavailableCause,
};
pub(super) use crate::check::checker::type_groups::{
    PublishedTypeGroupSurface, PublishedTypeGroupTerminal,
};
pub(super) use crate::check::checker::{
    check_bound_project_local_programs, BoundProjectBinder, ProjectLocalUserPassPlan,
};
pub(super) use crate::check::query::{SemanticQueryCoordinator, SemanticQueryState};
pub(super) use crate::class_semantics::{DemandOutcome, PublishedClasses};
pub(super) use crate::diagnostics::render_type;
use crate::diagnostics::{Diagnostic, IncompleteSurface, Severity};
pub(super) use crate::frontend::ProjectProgram;
pub(super) use crate::source::CompilationOrigin;
pub(super) use crate::source::LibraryFileOrdinal;
use crate::source::{ModuleOrdinal, UnitSlot};
pub(super) use crate::types::repr::{
    number_to_string, AliasId, ConditionalType, FunctionType, MappedType, ModifierOp, ObjectType,
    ParameterType, PropertyType, TemplateType, TupleRestType, TupleType, TypeTag, WellKnownSymbol,
};
pub(super) use crate::types::{
    substitute, GenericTypeParam, Interner, IntrinsicKind, LiteralValue, PropertyKey, TypeId,
    TypeParamId, Visibility,
};
pub(super) use rustc_hash::FxHashMap;
use std::collections::{BTreeMap, BTreeSet};
pub(super) use typokat_library_product::{
    sha256_hex, Accessibility, ContainerKind, DeclarationGroupRecipe, DeclarationKey,
    DeclarationPayload, DependencyKind, DependencyTarget, IntrinsicMarker, KeywordKind,
    LibraryDeclarationProduct, LiteralRecipe, MappedModifier, MemberKey, MemberKind, MemberRecipe,
    ParameterPattern, PropertyNameRecipe, QualifiedPathResolutionOutcome, QualifiedPathRole,
    QualifiedPathUnresolvedCause, RegionKey, SignatureKey, SourceSite, SymbolicValueNameRecipe,
    TypeOperator, TypeParameterKey, TypeRecipeKey, TypeRecipeNode, TypeReferenceResolution,
    ValueReferenceResolution,
};
pub(super) use typokat_types::alias_semantics::{
    PublishedAliasSurface, PublishedAliasTerminal, PublishedAliasUnavailable, PublishedAliases,
};

pub(super) fn well_known_symbol_from_name(name: &str) -> Option<WellKnownSymbol> {
    project_local_well_known_symbol_from_name(name)
}

pub(super) fn project_local_semantic_query_state(
    groups: [Option<TypeGroupId>; 8],
    published: &PublishedTypeEnvironment,
    interner: &Interner,
) -> SemanticQueryState {
    let identities =
        LibrarySemanticIdentities::select_project_local_exact(groups, published, interner.store());
    let mut state = SemanticQueryState::default();
    state.set_library_object_context(identities.object_relation_context());
    state.set_array_definitely_lacks_then(identities.array_definitely_lacks_then(interner.store()));
    state
}

pub(super) fn project_local_user_augmented_product_groups(
    bound: &BoundProjectBinder,
    mapped: &[ProjectLocalMappedProductGroupForTest],
) -> BTreeMap<usize, TypeGroupId> {
    mapped
        .iter()
        .filter_map(|row| {
            let local = row.local_type_group?;
            let group = TypeGroupId(u32::try_from(local).ok()?);
            let augmented = bound
                .binder
                .type_groups
                .get(group)?
                .fragments
                .iter()
                .any(|fragment| {
                    bound
                        .binder
                        .namespaces
                        .compilation_origin_for_source(fragment.source)
                        .is_some_and(|origin| matches!(origin, CompilationOrigin::User(_)))
                });
            augmented.then_some((row.product.product_key, group))
        })
        .collect()
}

pub(in crate::check::checker) fn project_local_final_product_dependencies(
    product: &LibraryDeclarationProduct,
    bound: &BoundProjectBinder,
    programs: &[ProjectProgram<'_>],
) -> Result<Vec<(usize, usize)>, &'static str> {
    let map = bound
        .library_product_map_for_test()
        .ok_or("project-local binder has no immutable-product identity map")?;
    let manifests = programs
        .iter()
        .map(|program| SourceDeclarationManifest::build(program.program))
        .collect::<Vec<_>>();
    let inventories = bound
        .module_scopes
        .iter()
        .zip(manifests)
        .map(|(module, manifest)| {
            let source = bound
                .binder
                .source_for_module(*module)
                .ok_or("project-local user source has no binder identity")?;
            crate::check::checker::decls::inventory::BoundTypeDeclarationInventory::build(
                &bound.binder,
                *module,
                source,
                manifest,
                None,
            )
            .map_err(|_| "project-local user declaration inventory is unavailable")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut dependencies =
        crate::check::checker::project_local_semantics::library_type_dependencies(product, map)?;
    dependencies.extend(
        crate::check::checker::project_local_semantics::user_type_dependencies(
            product,
            map,
            &bound.binder,
            &inventories,
        ),
    );
    let symbols = product
        .groups()
        .iter()
        .enumerate()
        .filter_map(|(index, _)| {
            let key = product.group_key_at(index)?;
            Some((map.group(key)?.symbol(), index))
        })
        .collect::<FxHashMap<_, _>>();
    let mut rows = dependencies
        .into_iter()
        .filter_map(|(owner, target)| Some((*symbols.get(&owner)?, *symbols.get(&target)?)))
        .collect::<Vec<_>>();
    rows.sort_unstable();
    rows.dedup();
    Ok(rows)
}

pub(super) fn project_local_final_type_group_fragment_counts(
    bound: &BoundProjectBinder,
    mapped: &[ProjectLocalMappedProductGroupForTest],
) -> BTreeMap<usize, usize> {
    mapped
        .iter()
        .filter_map(|row| {
            let local = row.local_type_group?;
            let group = TypeGroupId(u32::try_from(local).ok()?);
            Some((local, bound.binder.type_groups.get(group)?.fragments.len()))
        })
        .collect()
}

pub(super) fn project_local_final_value_declaration_counts(
    bound: &BoundProjectBinder,
    mapped: &[ProjectLocalMappedProductGroupForTest],
) -> BTreeMap<usize, usize> {
    mapped
        .iter()
        .filter_map(|row| {
            let value = row.local_value?;
            let symbol = SymbolId(u32::try_from(row.local_symbol).ok()?);
            Some((value, bound.binder.symbols.get(symbol)?.declarations.len()))
        })
        .collect()
}

#[derive(Clone)]
pub(super) struct ProjectLocalTypeProbeCoordinate {
    pub(super) identity: usize,
    pub(super) product_keys: Vec<usize>,
    pub(super) name: String,
    pub(super) source_path: Option<String>,
}

#[derive(Clone)]
pub(super) struct ProjectLocalValueProbeCoordinate {
    pub(super) identity: usize,
    pub(super) product_keys: Vec<usize>,
    pub(super) namespace_product_keys: Vec<usize>,
    pub(super) name: String,
}

pub(super) struct ProjectLocalProbeAuthority {
    pub(super) types: Vec<ProjectLocalTypeProbeCoordinate>,
    pub(super) values: Vec<ProjectLocalValueProbeCoordinate>,
}

pub(super) fn project_local_probe_authority(
    product: &LibraryDeclarationProduct,
    bound: &BoundProjectBinder,
) -> Option<ProjectLocalProbeAuthority> {
    let map = bound.library_product_map.as_ref()?;
    let product_keys_for = |scope: ScopeId,
                            ty: Option<TypeGroupId>,
                            value: Option<ValueStorageId>,
                            symbol: Option<SymbolId>,
                            coordinate_name: &str| {
        product
            .groups()
            .iter()
            .enumerate()
            .filter_map(|(index, group)| {
                let key = product.group_key_at(index)?;
                let binding = map.group(key)?;
                (map.container(group.container) == Some(scope)
                    && product.strings()[group.name.index()] == coordinate_name
                    && ty.is_none_or(|identity| binding.ty() == Some(identity))
                    && value.is_none_or(|identity| binding.value() == Some(identity))
                    && symbol.is_none_or(|identity| binding.symbol() == identity))
                .then_some(index)
            })
            .collect::<Vec<_>>()
    };
    let mut types = Vec::new();
    for group in bound.binder.type_groups.iter() {
        let visible = bound
            .binder
            .graph
            .get(bound.binder.compilation_global)
            .and_then(|scope| scope.lookup_local(&group.name))
            .and_then(|symbol| bound.binder.symbols.get(symbol))
            .is_some_and(|symbol| symbol.ty == Some(group.id));
        if visible {
            types.push(ProjectLocalTypeProbeCoordinate {
                identity: group.id.index(),
                product_keys: product_keys_for(
                    bound.binder.compilation_global,
                    Some(group.id),
                    None,
                    None,
                    &group.name,
                )
                .into_iter()
                .filter(|key| product.groups()[*key].slots.r#type)
                .collect(),
                name: group.name.clone(),
                source_path: None,
            });
        }
    }
    for (source_index, source) in product.sources().iter().enumerate() {
        let source_key = product.source_key_at(source_index)?;
        let module = map.source(source_key)?.module();
        for group in bound.binder.type_groups.iter() {
            let visible = bound
                .binder
                .graph
                .get(module)
                .and_then(|scope| scope.lookup_local(&group.name))
                .and_then(|symbol| bound.binder.symbols.get(symbol))
                .is_some_and(|symbol| symbol.ty == Some(group.id));
            if visible {
                types.push(ProjectLocalTypeProbeCoordinate {
                    identity: group.id.index(),
                    product_keys: product_keys_for(module, Some(group.id), None, None, &group.name)
                        .into_iter()
                        .filter(|key| product.groups()[*key].slots.r#type)
                        .collect(),
                    name: group.name.clone(),
                    source_path: Some(product.strings()[source.name.index()].clone()),
                });
            }
        }
    }
    let mut values = Vec::new();
    for (symbol_id, symbol) in bound.binder.symbols.iter() {
        let visible = bound
            .binder
            .graph
            .get(bound.binder.compilation_global)
            .and_then(|scope| scope.lookup_local(&symbol.name))
            == Some(symbol_id);
        if visible {
            if let Some(identity) = symbol.value {
                values.push(ProjectLocalValueProbeCoordinate {
                    identity: identity.index(),
                    product_keys: product_keys_for(
                        bound.binder.compilation_global,
                        None,
                        Some(identity),
                        Some(symbol_id),
                        &symbol.name,
                    )
                    .into_iter()
                    .filter(|key| product.groups()[*key].slots.value)
                    .collect(),
                    namespace_product_keys: product_keys_for(
                        bound.binder.compilation_global,
                        None,
                        Some(identity),
                        Some(symbol_id),
                        &symbol.name,
                    )
                    .into_iter()
                    .filter(|key| product.groups()[*key].slots.namespace)
                    .collect(),
                    name: symbol.name.clone(),
                });
            }
        }
    }
    Some(ProjectLocalProbeAuthority { types, values })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignatureProbe {
    pub parameter_types: Vec<String>,
    pub return_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NormalizedSpanForTest {
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NormalizedDiagnosticForTest {
    pub span: NormalizedSpanForTest,
    pub code: String,
    pub severity: String,
    pub message: String,
    pub elaboration: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NormalizedIncompleteForTest {
    pub span: NormalizedSpanForTest,
    pub id: String,
    pub context: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NormalizedCheckerRecordForTest {
    Diagnostic(NormalizedDiagnosticForTest),
    Incomplete(NormalizedIncompleteForTest),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedUserEventRecordForTest {
    pub source_ordinal: LibraryFileOrdinal,
    pub module_ordinal: ModuleOrdinal,
    pub source_start: u32,
    pub event_ordinal: usize,
    pub record_ordinal: usize,
    pub record: NormalizedCheckerRecordForTest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedLibraryRecordForTest {
    pub source_ordinal: LibraryFileOrdinal,
    pub owner_start: u32,
    pub record: NormalizedCheckerRecordForTest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedCheckResultForTest {
    pub source_ordinal: LibraryFileOrdinal,
    pub module_ordinal: ModuleOrdinal,
    pub unit_slot: UnitSlot,
    pub diagnostics: Vec<NormalizedDiagnosticForTest>,
    pub incomplete: Vec<NormalizedIncompleteForTest>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedTypeProbeForTest {
    pub name: String,
    pub declaration_count: usize,
    pub member_names: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedModuleTypeProbeForTest {
    pub source_ordinal: LibraryFileOrdinal,
    pub probe: NormalizedTypeProbeForTest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedCallableMemberProbeForTest {
    pub name: String,
    pub source_ordinal: LibraryFileOrdinal,
    pub reservation_source_ordinal: LibraryFileOrdinal,
    pub source_start: u32,
    pub call_signature_count: usize,
    pub signatures: Vec<SignatureProbe>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedValueProbeForTest {
    pub name: String,
    pub declaration_count: usize,
    pub call_signature_count: usize,
    pub signatures: Vec<SignatureProbe>,
    pub member_names: Vec<String>,
    pub callable_members: Vec<NormalizedCallableMemberProbeForTest>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalizedSemanticProbesForTest {
    pub global_types: Vec<NormalizedTypeProbeForTest>,
    pub module_types: Vec<NormalizedModuleTypeProbeForTest>,
    pub global_values: Vec<NormalizedValueProbeForTest>,
}

pub(super) fn project_local_published_types(
    product: &LibraryDeclarationProduct,
    mapped: &[ProjectLocalMappedProductGroupForTest],
    group_types: &[Option<TypeId>],
    type_parameter_ids: &[TypeParamId],
    type_parameter_defaults: &[PublishedTypeParameterDefault],
    ready_product_groups: &BTreeSet<usize>,
    unavailable_product_groups: &BTreeSet<usize>,
    aliases: PublishedAliases,
) -> PublishedTypeEnvironment {
    let terminal_count = mapped
        .iter()
        .filter_map(|row| row.local_type_group)
        .max()
        .map_or(0, |index| index.saturating_add(1));
    let unavailable = PublishedTypeGroupTerminal::Unavailable(PublishedTypeGroupUnavailable {
        cause: TypeGroupUnavailableCause::UnsupportedComposition,
    });
    let mut terminals = (0..terminal_count)
        .map(|_| unavailable.clone())
        .collect::<Vec<_>>();
    for row in mapped {
        if !ready_product_groups.contains(&row.product.product_key)
            || unavailable_product_groups.contains(&row.product.product_key)
        {
            continue;
        }
        let (Some(local), Some(ty)) = (
            row.local_type_group,
            group_types.get(row.product.product_key).copied().flatten(),
        ) else {
            continue;
        };
        let product_group = &product.groups()[row.product.product_key];
        let parameters = product_group
            .fragments
            .iter()
            .find_map(
                |declaration| match &product.declarations()[declaration.index()].payload {
                    DeclarationPayload::Class {
                        type_parameters, ..
                    }
                    | DeclarationPayload::Interface {
                        type_parameters, ..
                    }
                    | DeclarationPayload::TypeAlias {
                        type_parameters, ..
                    } => Some(type_parameters.as_ref()),
                    _ => None,
                },
            )
            .unwrap_or_default();
        let parameter_ids = parameters
            .iter()
            .map(|parameter| type_parameter_ids[parameter.index()])
            .collect::<Vec<_>>();
        let parameter_names = parameters
            .iter()
            .map(|parameter| {
                product.strings()[product.type_parameters()[parameter.index()].name.index()].clone()
            })
            .collect::<Vec<_>>();
        let parameter_defaults = parameters
            .iter()
            .map(|parameter| type_parameter_defaults[parameter.index()])
            .collect::<Vec<_>>();
        terminals[local] = PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
            name: row.product.name.clone(),
            surface: PublishedTypeGroupSurface::Template(ty),
            parameters: parameter_ids,
            parameter_names,
            parameter_defaults,
            conflict_alternatives: Vec::new(),
        });
    }
    PublishedTypeEnvironment::from_project_local_terminals_with_aliases(terminals, aliases)
}

pub(super) struct ProjectLocalUserPassObservables {
    pub(super) event_ledger: Vec<ProjectLocalUserEventRecordForTest>,
    pub(super) check_results: Vec<ProjectLocalCheckResultForTest>,
    pub(super) unavailable_audits: Vec<ProjectLocalUnavailableAuditForTest>,
    pub(super) published_type_groups: Vec<PublishedTypeGroupTerminal>,
    pub(super) library_records: Vec<(usize, super::ProjectLocalLibraryRecordForTest)>,
}

pub(super) struct ProjectLocalUserPassRequest<'a, 'ast> {
    pub(super) bound: BoundProjectBinder,
    pub(super) programs: &'a [ProjectProgram<'ast>],
    pub(super) published_types: PublishedTypeEnvironment,
    pub(super) product_value_types: Vec<(ValueStorageId, TypeId)>,
    pub(super) product_namespace_seeds: Vec<ProjectLocalNamespaceSeed>,
    pub(super) product_type_overlays: Vec<(TypeGroupId, ProjectLocalTypeOverlayDraft)>,
    pub(super) user_namespace_demands: Vec<String>,
    pub(super) library_identity_groups: [Option<TypeGroupId>; 8],
    pub(super) next_type_param: u32,
    pub(super) lexical_array_alias: Option<TypeGroupId>,
    pub(super) demand_gateway: &'a mut dyn ProjectLocalUserDemandGateway,
    pub(super) library_validations: Vec<ProjectLocalValidationCandidate>,
    pub(super) library_validation_groups: BTreeMap<usize, TypeGroupId>,
    pub(super) library_source_paths: BTreeMap<LibraryFileOrdinal, String>,
    pub(super) fault: Option<ProjectLocalSemanticFaultForTest>,
}

pub(super) fn run_project_local_user_pass<'ast>(
    interner: &mut Interner,
    request: ProjectLocalUserPassRequest<'_, 'ast>,
) -> Result<ProjectLocalUserPassObservables, &'static str> {
    let ProjectLocalUserPassRequest {
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
        demand_gateway,
        library_validations,
        library_validation_groups,
        library_source_paths,
        fault,
    } = request;
    let output = check_bound_project_local_programs(
        interner,
        programs,
        ProjectLocalUserPassPlan {
            bound,
            published_types,
            product_value_types,
            product_namespace_seeds,
            product_type_overlays,
            user_namespace_demands,
            library_identity_groups,
            next_type_param,
            lexical_array_alias,
            demand_gateway,
            library_validations,
            library_validation_groups,
            inject_provisional_unavailable_write: fault
                == Some(ProjectLocalSemanticFaultForTest::PublishProvisionalUnavailableWrite),
        },
    )?;
    if output
        .unavailable_namespaces
        .iter()
        .any(|namespace| !namespace.published_provisional_writes.is_empty())
    {
        let reached = super::fault_gate::activate_if(
            fault,
            ProjectLocalSemanticFaultForTest::PublishProvisionalUnavailableWrite,
        );
        debug_assert!(reached);
    }
    let event_ledger = output
        .records
        .iter()
        .map(|(key, record)| {
            let source_path = programs
                .iter()
                .find(|program| program.module_ordinal == key.module_ordinal)
                .ok_or("project-local event has no source program")?
                .normalized_path
                .clone();
            Ok(ProjectLocalUserEventRecordForTest {
                source_path,
                source_start: key.source_start,
                event_ordinal: key.event_ordinal,
                record_ordinal: key.record_ordinal,
                record: normalize_checker_record(record),
            })
        })
        .collect::<Result<Vec<_>, &'static str>>()?;
    let check_results = output
        .results
        .iter()
        .map(|result| {
            let source_path = programs
                .iter()
                .find(|program| program.module_ordinal == result.module_ordinal)
                .ok_or("project-local check result has no source program")?
                .normalized_path
                .clone();
            Ok(ProjectLocalCheckResultForTest {
                source_path,
                diagnostics: result
                    .diagnostics
                    .iter()
                    .map(normalize_diagnostic)
                    .collect(),
                incomplete: result.incomplete.iter().map(normalize_incomplete).collect(),
            })
        })
        .collect::<Result<Vec<_>, &'static str>>()?;
    let unavailable_audits = output
        .unavailable_namespaces
        .into_iter()
        .map(|namespace| {
            let fragments = namespace
                .fragments
                .into_iter()
                .map(|(origin, source_start)| {
                    let source_path = programs
                        .iter()
                        .find(|program| program.compilation_unit.origin == origin)
                        .ok_or("project-local namespace fragment has no source program")?
                        .normalized_path
                        .clone();
                    Ok(ProjectLocalNamespaceFragmentForTest {
                        source_path,
                        origin: ProjectLocalContributorOriginForTest::User,
                        source_start,
                    })
                })
                .collect::<Result<Vec<_>, &'static str>>()?;
            let reason = match namespace.reason {
                crate::check::checker::ProjectLocalUserNamespaceUnavailableReason::Unsupported => {
                    ProjectLocalUnavailableReasonForTest::UnsupportedUserDeclaration
                }
                crate::check::checker::ProjectLocalUserNamespaceUnavailableReason::Invalid => {
                    ProjectLocalUnavailableReasonForTest::InvalidUserDeclaration
                }
                crate::check::checker::ProjectLocalUserNamespaceUnavailableReason::Cyclic => {
                    ProjectLocalUnavailableReasonForTest::CyclicUserDeclaration
                }
            };
            Ok(ProjectLocalUnavailableAuditForTest {
                terminal: ProjectLocalUnavailableTerminalForTest {
                    name: namespace.name,
                    fragments,
                    reason,
                },
                discarded_provisional_writes: namespace.discarded_provisional_writes,
                published_provisional_writes: namespace.published_provisional_writes,
            })
        })
        .collect::<Result<Vec<_>, &'static str>>()?;
    let library_records = output
        .library_records
        .iter()
        .map(|(group, key, record)| {
            let source_path = library_source_paths
                .get(&key.file_ordinal)
                .cloned()
                .ok_or("project-local library validation source path is unavailable")?;
            Ok((
                *group,
                super::ProjectLocalLibraryRecordForTest {
                    source_path,
                    owner_start: key.source_start,
                    record: normalize_checker_record(record),
                },
            ))
        })
        .collect::<Result<Vec<_>, &'static str>>()?;
    Ok(ProjectLocalUserPassObservables {
        event_ledger,
        check_results,
        unavailable_audits,
        published_type_groups: output.published_type_groups,
        library_records,
    })
}

fn normalize_diagnostic(diagnostic: &Diagnostic) -> NormalizedDiagnosticForTest {
    NormalizedDiagnosticForTest {
        span: NormalizedSpanForTest {
            start: diagnostic.span.start,
            end: diagnostic.span.end,
        },
        code: diagnostic.code.as_str().to_owned(),
        severity: match diagnostic.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
        .to_owned(),
        message: diagnostic.message.clone(),
        elaboration: diagnostic.elaboration().to_vec(),
    }
}

fn normalize_incomplete(incomplete: &IncompleteSurface) -> NormalizedIncompleteForTest {
    NormalizedIncompleteForTest {
        span: NormalizedSpanForTest {
            start: incomplete.span.start,
            end: incomplete.span.end,
        },
        id: incomplete.id.clone(),
        context: incomplete.context.clone(),
    }
}

fn normalize_checker_record(record: &CheckerRecord) -> NormalizedCheckerRecordForTest {
    match record {
        CheckerRecord::Diagnostic(diagnostic) => {
            NormalizedCheckerRecordForTest::Diagnostic(normalize_diagnostic(diagnostic))
        }
        CheckerRecord::Incomplete(incomplete) => {
            NormalizedCheckerRecordForTest::Incomplete(normalize_incomplete(incomplete))
        }
    }
}

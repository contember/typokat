use super::bind::Binder;
use super::declaration::{
    DeclarationKind, SourceDeclarationManifest, SourceDeclarationProductPayload,
    SourceInterfaceOccurrenceKind, TypeGroupId, ValueStorageId,
};
use super::namespace::{
    NamespaceId, NamespaceOwner, QualifiedTypePathResolution, QualifiedValuePathResolution,
};
use super::scope::{ScopeId, ScopeKind};
use crate::span::Span;
use oxc_ast::ast::{
    FormalParameters, PropertyKey, TSLiteral, TSMappedTypeModifierOperator, TSSignature,
    TSTupleElement, TSType, TSTypeName, TSTypeOperatorOperator, TSTypeParameterDeclaration,
};
use oxc_span::GetSpan;
use std::collections::BTreeMap;
use typokat_library_product as product;

pub struct LibraryProductSource<'manifest, 'ast> {
    pub ordinal: u32,
    pub name: &'manifest str,
    pub source_sha256: &'manifest str,
    pub byte_len: u32,
    pub external_module: bool,
    pub module: ScopeId,
    pub manifest: &'manifest SourceDeclarationManifest<'ast>,
    pub source_text: &'manifest str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryProductProjectionError {
    EmptySources,
    NonCanonicalSourceOrder {
        row: usize,
    },
    MissingAuthenticatedDeclaration {
        source: u32,
        binding_start: u32,
    },
    MissingAuthenticatedScope {
        source: u32,
        binding_start: u32,
    },
    AmbiguousAuthenticatedScope {
        source: u32,
        scope: u32,
    },
    MissingDependencyOwner {
        source: u32,
        start: u32,
    },
    AuthenticatedSiteMismatch {
        source: u32,
        binding_start: u32,
    },
    MissingPayload {
        source: u32,
        binding_start: u32,
    },
    UnsupportedType {
        source: u32,
        start: u32,
        form: &'static str,
    },
    UnsupportedProperty {
        source: u32,
        start: u32,
    },
    SourceSlice {
        source: u32,
        start: u32,
        end: u32,
    },
    Product(product::ProductBuildError),
    AuthenticatedOracleMismatch {
        table: &'static str,
    },
}

impl std::fmt::Display for LibraryProductProjectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "library declaration projection failed: {self:?}")
    }
}
impl std::error::Error for LibraryProductProjectionError {}
impl From<product::ProductBuildError> for LibraryProductProjectionError {
    fn from(value: product::ProductBuildError) -> Self {
        Self::Product(value)
    }
}

pub fn project_library_declaration_product(
    profile_identity: &str,
    binder: &Binder,
    sources: &[LibraryProductSource<'_, '_>],
) -> Result<product::LibraryDeclarationProduct, LibraryProductProjectionError> {
    if sources.is_empty() {
        return Err(LibraryProductProjectionError::EmptySources);
    }
    for row in 1..sources.len() {
        if sources[row - 1].ordinal >= sources[row].ordinal {
            return Err(LibraryProductProjectionError::NonCanonicalSourceOrder { row });
        }
    }
    let mut assembly = product::ProductAssembly::new();
    let global_container = assembly.add_container(product::ContainerRecipe {
        parent: None,
        kind: product::ContainerKind::CompilationGlobal,
        name: None,
        anchor: None,
    })?;
    let mut container_parents = BTreeMap::new();
    let mut container_segments = BTreeMap::new();
    container_parents.insert(global_container, None);
    container_segments.insert(
        global_container,
        StableContainerSegment {
            kind: "compilation-global",
            name: None,
            anchor: None,
        },
    );
    let mut source_regions = Vec::with_capacity(sources.len());
    let mut source_containers = Vec::with_capacity(sources.len());
    let mut scope_containers = BTreeMap::new();
    let mut stable_region_anchors = BTreeMap::new();
    for input in sources {
        let source_key = assembly.next_source_key()?;
        let source_container = if input.external_module {
            let container = assembly.add_container(product::ContainerRecipe {
                parent: None,
                kind: product::ContainerKind::ExternalModule,
                name: None,
                anchor: Some(product::SourceSite {
                    source: source_key,
                    start: 0,
                    end: input.byte_len,
                }),
            })?;
            container_parents.insert(container, None);
            container_segments.insert(
                container,
                StableContainerSegment {
                    kind: "external-module",
                    name: None,
                    anchor: Some(StableSourceSite {
                        source_ordinal: input.ordinal,
                        start: 0,
                        end: input.byte_len,
                    }),
                },
            );
            container
        } else {
            global_container
        };
        scope_containers.insert(input.module.0, source_container);
        let region_key = assembly.next_region_key()?;
        let name = assembly.intern(input.name)?;
        let sha256 = assembly.intern(input.source_sha256)?;
        assembly.add_source(product::SourceRecipe {
            ordinal: input.ordinal,
            name,
            sha256,
            byte_len: input.byte_len,
            classification: if input.external_module {
                product::SourceClassification::ExternalModule
            } else {
                product::SourceClassification::Script
            },
            root_region: region_key,
        })?;
        assembly.add_region(product::RegionRecipe {
            parent: None,
            container: source_container,
            kind: product::RegionKind::Source,
            site: product::SourceSite {
                source: source_key,
                start: 0,
                end: input.byte_len,
            },
        })?;
        stable_region_anchors.insert(
            region_key,
            StableRegionAnchor {
                site: StableSourceSite {
                    source_ordinal: input.ordinal,
                    start: 0,
                    end: input.byte_len,
                },
                kind: region_kind_name(product::RegionKind::Source),
            },
        );
        source_regions.push(region_key);
        source_containers.push(source_container);
    }
    scope_containers.insert(binder.compilation_global.0, global_container);
    scope_containers.insert(binder.script_namespace_root.0, global_container);

    let mut namespace_facts = BTreeMap::<NamespaceId, NamespaceContainerFact>::new();
    for input in sources {
        for occurrence in input.manifest.declarations() {
            let Some(bound) = binder.declarations.declaration_at_site(
                input.module,
                occurrence.binding_span.start,
                occurrence.kind,
            ) else {
                continue;
            };
            let Some(namespace) = bound.namespace else {
                continue;
            };
            let is_ambient_external = matches!(
                occurrence.product_payload,
                Some(SourceDeclarationProductPayload::Namespace(value))
                    if matches!(value.id, oxc_ast::ast::TSModuleDeclarationName::StringLiteral(_))
            );
            let anchor = StableSourceSite {
                source_ordinal: input.ordinal,
                start: occurrence.binding_span.start,
                end: occurrence.binding_span.end,
            };
            namespace_facts
                .entry(namespace)
                .and_modify(|fact| {
                    if anchor < fact.anchor {
                        fact.anchor = anchor;
                    }
                    fact.ambient_external |= is_ambient_external;
                })
                .or_insert(NamespaceContainerFact {
                    anchor,
                    ambient_external: is_ambient_external,
                });
        }
    }
    let mut namespace_order = namespace_facts
        .iter()
        .map(|(namespace, fact)| (*namespace, fact.clone()))
        .collect::<Vec<_>>();
    namespace_order.sort_by_key(|(_, fact)| fact.anchor);
    let mut namespace_containers = BTreeMap::new();
    for (namespace_id, fact) in &namespace_order {
        let namespace = binder.namespaces.get(*namespace_id).ok_or(
            LibraryProductProjectionError::MissingAuthenticatedScope {
                source: fact.anchor.source_ordinal,
                binding_start: fact.anchor.start,
            },
        )?;
        let source = sources
            .iter()
            .position(|input| input.ordinal == fact.anchor.source_ordinal)
            .and_then(|index| assembly.source_key_at(index))
            .ok_or(LibraryProductProjectionError::MissingAuthenticatedScope {
                source: fact.anchor.source_ordinal,
                binding_start: fact.anchor.start,
            })?;
        let name = assembly.intern(&namespace.name)?;
        let key = assembly.add_container(product::ContainerRecipe {
            parent: None,
            kind: if fact.ambient_external {
                product::ContainerKind::AmbientExternalModule
            } else {
                product::ContainerKind::Namespace
            },
            name: Some(name),
            anchor: Some(product::SourceSite {
                source,
                start: fact.anchor.start,
                end: fact.anchor.end,
            }),
        })?;
        container_parents.insert(key, None);
        container_segments.insert(
            key,
            StableContainerSegment {
                kind: if fact.ambient_external {
                    "ambient-external-module"
                } else {
                    "namespace"
                },
                name: Some(namespace.name.clone()),
                anchor: Some(fact.anchor),
            },
        );
        namespace_containers.insert(*namespace_id, key);
        scope_containers.insert(namespace.public_scope.0, key);
    }
    for (namespace_id, _) in &namespace_order {
        let namespace = binder.namespaces.get(*namespace_id).ok_or(
            LibraryProductProjectionError::MissingAuthenticatedScope {
                source: 0,
                binding_start: 0,
            },
        )?;
        let parent = container_for_namespace_owner(
            binder,
            namespace.owner,
            global_container,
            &scope_containers,
            &namespace_containers,
        )?;
        assembly.set_container_parent(namespace_containers[namespace_id], parent)?;
        container_parents.insert(namespace_containers[namespace_id], Some(parent));
    }
    let stable_container_anchors = container_segments
        .keys()
        .copied()
        .map(|key| {
            Ok((
                key,
                stable_container_from_parts(key, &container_parents, &container_segments)?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, LibraryProductProjectionError>>()?;
    let source = assembly
        .source_key_at(0)
        .ok_or(LibraryProductProjectionError::EmptySources)?;
    let mut context = ProjectionContext {
        binder,
        assembly,
        source,
        source_ordinal: 0,
        group_keys: BTreeMap::new(),
        value_group_keys: BTreeMap::new(),
        type_parameters: BTreeMap::new(),
        type_by_site: BTreeMap::new(),
        pending_dependencies: Vec::new(),
        current_group: None,
        current_scope: None,
        current_region: source_regions[0],
        callable_scope_keys: BTreeMap::new(),
        active_type_parameters: Vec::new(),
        stable_group_anchors: BTreeMap::new(),
        stable_type_parameter_anchors: BTreeMap::new(),
        stable_region_anchors,
        stable_container_anchors,
        global_container,
        namespace_containers,
        region_containers: source_regions
            .iter()
            .copied()
            .zip(source_containers.iter().copied())
            .collect(),
    };
    for group in binder.type_groups.iter().filter(|group| {
        group.fragments.iter().any(|fragment| {
            sources
                .iter()
                .any(|source| source.module == fragment.site.module)
        })
    }) {
        let name = context.assembly.intern(&group.name)?;
        let key = context
            .assembly
            .add_group(product::DeclarationGroupRecipe {
                container: global_container,
                name,
                slots: product::DeclarationSlots {
                    value: false,
                    r#type: true,
                    namespace: false,
                },
                fragments: Box::new([]),
            })?;
        context.group_keys.insert(group.id, key);
    }
    let mut extra_group_members = BTreeMap::<(u32, String), Vec<u32>>::new();
    for input in sources {
        for occurrence in input.manifest.declarations() {
            let Some(bound) = binder.declarations.declaration_at_site(
                input.module,
                occurrence.binding_span.start,
                occurrence.kind,
            ) else {
                continue;
            };
            if bound.type_group.is_some()
                || (bound.value_storage.is_none() && bound.namespace.is_none())
            {
                continue;
            }
            let name = input
                .source_text
                .get(
                    usize::try_from(occurrence.binding_span.start).unwrap_or(usize::MAX)
                        ..usize::try_from(occurrence.binding_span.end).unwrap_or(usize::MAX),
                )
                .ok_or(LibraryProductProjectionError::SourceSlice {
                    source: input.ordinal,
                    start: occurrence.binding_span.start,
                    end: occurrence.binding_span.end,
                })?;
            extra_group_members
                .entry((
                    bound
                        .site
                        .scope
                        .map(|scope| scope.0)
                        .unwrap_or(input.module.0),
                    name.to_owned(),
                ))
                .or_default()
                .push(bound.id.0);
        }
    }
    let mut extra_group_by_declaration = BTreeMap::new();
    for ((_scope, name), members) in &extra_group_members {
        let name = context.assembly.intern(name)?;
        let key = context
            .assembly
            .add_group(product::DeclarationGroupRecipe {
                container: global_container,
                name,
                slots: product::DeclarationSlots::default(),
                fragments: Box::new([]),
            })?;
        for member in members {
            extra_group_by_declaration.insert(*member, key);
        }
    }
    for declaration in binder.declarations.iter() {
        let group = declaration
            .type_group
            .and_then(|group| context.group_keys.get(&group).copied())
            .or_else(|| extra_group_by_declaration.get(&declaration.id.0).copied());
        if let (Some(storage), Some(group)) = (declaration.value_storage, group) {
            context.value_group_keys.insert(storage.0, group);
        }
    }
    let mut declaration_keys = BTreeMap::new();
    let mut declaration_facts = Vec::new();
    for (source_index, (input, root_region)) in sources.iter().zip(&source_regions).enumerate() {
        context.source = context
            .assembly
            .source_key_at(source_index)
            .ok_or(LibraryProductProjectionError::EmptySources)?;
        context.source_ordinal = input.ordinal;
        let mut child_regions = BTreeMap::new();
        let mut region_ranges = Vec::new();
        let mut lexical_regions = BTreeMap::new();
        insert_lexical_region(
            &mut lexical_regions,
            binder.statement_lexical_root(input.module),
            *root_region,
            input.ordinal,
        )?;
        for occurrence in input.manifest.declarations() {
            let (kind, span, child_container) = match occurrence.product_payload {
                Some(SourceDeclarationProductPayload::Namespace(value)) => {
                    let Some(body) = value.body.as_ref() else {
                        continue;
                    };
                    let kind = if matches!(
                        value.id,
                        oxc_ast::ast::TSModuleDeclarationName::StringLiteral(_)
                    ) {
                        product::RegionKind::AmbientExternalModule
                    } else {
                        product::RegionKind::Namespace
                    };
                    let bound = binder
                        .declarations
                        .declaration_at_site(
                            input.module,
                            occurrence.binding_span.start,
                            occurrence.kind,
                        )
                        .ok_or(
                            LibraryProductProjectionError::MissingAuthenticatedDeclaration {
                                source: input.ordinal,
                                binding_start: occurrence.binding_span.start,
                            },
                        )?;
                    let namespace = bound.namespace.ok_or(
                        LibraryProductProjectionError::MissingAuthenticatedScope {
                            source: input.ordinal,
                            binding_start: occurrence.binding_span.start,
                        },
                    )?;
                    let container = context
                        .namespace_containers
                        .get(&namespace)
                        .copied()
                        .ok_or(LibraryProductProjectionError::MissingAuthenticatedScope {
                            source: input.ordinal,
                            binding_start: occurrence.binding_span.start,
                        })?;
                    (kind, body.span(), container)
                }
                Some(SourceDeclarationProductPayload::Global(value)) => (
                    product::RegionKind::GlobalAugmentation,
                    value.body.span,
                    context.global_container,
                ),
                _ => continue,
            };
            let parent = region_ranges
                .iter()
                .filter(|(start, end, _, _)| *start <= span.start && span.end <= *end)
                .min_by_key(|(start, end, _, _)| end.saturating_sub(*start))
                .map(|(_, _, _, key)| *key)
                .unwrap_or(*root_region);
            let key = context.assembly.add_region(product::RegionRecipe {
                parent: Some(parent),
                container: child_container,
                kind,
                site: context.oxc_site(span),
            })?;
            context.stable_region_anchors.insert(
                key,
                StableRegionAnchor {
                    site: StableSourceSite {
                        source_ordinal: input.ordinal,
                        start: span.start,
                        end: span.end,
                    },
                    kind: region_kind_name(kind),
                },
            );
            child_regions.insert(occurrence.binding_span.start, key);
            context.region_containers.insert(key, child_container);
            region_ranges.push((span.start, span.end, occurrence.binding_span.start, key));
            match kind {
                product::RegionKind::Namespace | product::RegionKind::AmbientExternalModule => {
                    let private_scope = binder
                        .namespace_fragment_private_scope(
                            input.module,
                            occurrence.declaration_span.start,
                        )
                        .ok_or(LibraryProductProjectionError::MissingAuthenticatedScope {
                            source: input.ordinal,
                            binding_start: occurrence.binding_span.start,
                        })?;
                    insert_lexical_region(&mut lexical_regions, private_scope, key, input.ordinal)?;
                }
                product::RegionKind::GlobalAugmentation => {
                    let overlay_scope = binder
                        .global_augmentation_scope(input.module, occurrence.binding_span.start)
                        .ok_or(LibraryProductProjectionError::MissingAuthenticatedScope {
                            source: input.ordinal,
                            binding_start: occurrence.binding_span.start,
                        })?;
                    let body_scope = binder
                        .global_augmentation_body_scope(input.module, occurrence.binding_span.start)
                        .ok_or(LibraryProductProjectionError::MissingAuthenticatedScope {
                            source: input.ordinal,
                            binding_start: occurrence.binding_span.start,
                        })?;
                    insert_lexical_region(&mut lexical_regions, overlay_scope, key, input.ordinal)?;
                    insert_lexical_region(&mut lexical_regions, body_scope, key, input.ordinal)?;
                }
                product::RegionKind::Source => {}
            }
        }
        let mut callable_facts = Vec::new();
        for occurrence in input.manifest.declarations() {
            let bound = binder
                .declarations
                .declaration_at_site(input.module, occurrence.binding_span.start, occurrence.kind)
                .ok_or(
                    LibraryProductProjectionError::MissingAuthenticatedDeclaration {
                        source: input.ordinal,
                        binding_start: occurrence.binding_span.start,
                    },
                )?;
            match occurrence.product_payload {
                Some(SourceDeclarationProductPayload::Function(function)) => {
                    callable_facts.push((function.span, bound.site.scope));
                }
                Some(SourceDeclarationProductPayload::Class(class)) => {
                    for element in &class.body.body {
                        if let oxc_ast::ast::ClassElement::MethodDefinition(method) = element {
                            callable_facts.push((method.value.span, bound.site.scope));
                        }
                    }
                }
                Some(SourceDeclarationProductPayload::Variable(_))
                | Some(SourceDeclarationProductPayload::Catch(_))
                | Some(SourceDeclarationProductPayload::Parameter(_))
                | Some(SourceDeclarationProductPayload::RestParameter(_))
                | Some(SourceDeclarationProductPayload::TypeAlias(_))
                | Some(SourceDeclarationProductPayload::Interface(_))
                | Some(SourceDeclarationProductPayload::Enum(_))
                | Some(SourceDeclarationProductPayload::Namespace(_))
                | Some(SourceDeclarationProductPayload::Global(_))
                | None => {}
            }
        }
        callable_facts.sort_by_key(|(span, _)| (span.start, span.end));
        callable_facts.dedup_by_key(|(span, _)| *span);
        for (span, expected_parent_scope) in callable_facts {
            let scope = binder
                .fn_scopes
                .get(&(input.module, span.start))
                .copied()
                .ok_or(LibraryProductProjectionError::MissingAuthenticatedScope {
                    source: input.ordinal,
                    binding_start: span.start,
                })?;
            let parent_scope = binder.graph.get(scope).and_then(|row| row.parent).ok_or(
                LibraryProductProjectionError::MissingAuthenticatedScope {
                    source: input.ordinal,
                    binding_start: span.start,
                },
            )?;
            if expected_parent_scope != Some(parent_scope) {
                return Err(LibraryProductProjectionError::AuthenticatedSiteMismatch {
                    source: input.ordinal,
                    binding_start: span.start,
                });
            }
            let parent = if let Some(parent) = context.callable_scope_keys.get(&parent_scope.0) {
                product::LexicalScopeParent::Callable(*parent)
            } else {
                let region = lexical_regions.get(&parent_scope.0).copied().ok_or(
                    LibraryProductProjectionError::MissingAuthenticatedScope {
                        source: input.ordinal,
                        binding_start: span.start,
                    },
                )?;
                product::LexicalScopeParent::Region(region)
            };
            let key = context
                .assembly
                .add_callable_scope(product::CallableScopeRecipe {
                    site: context.oxc_site(span),
                    parent,
                })?;
            context.callable_scope_keys.insert(scope.0, key);
        }
        for occurrence in input.manifest.declarations() {
            let Some(bound) = binder.declarations.declaration_at_site(
                input.module,
                occurrence.binding_span.start,
                occurrence.kind,
            ) else {
                return Err(
                    LibraryProductProjectionError::MissingAuthenticatedDeclaration {
                        source: input.ordinal,
                        binding_start: occurrence.binding_span.start,
                    },
                );
            };
            if bound.site.declaration_span != occurrence.declaration_span
                || bound.site.binding_span != occurrence.binding_span
            {
                return Err(LibraryProductProjectionError::AuthenticatedSiteMismatch {
                    source: input.ordinal,
                    binding_start: occurrence.binding_span.start,
                });
            }
            let name_text = input
                .source_text
                .get(
                    usize::try_from(occurrence.binding_span.start).unwrap_or(usize::MAX)
                        ..usize::try_from(occurrence.binding_span.end).unwrap_or(usize::MAX),
                )
                .ok_or(LibraryProductProjectionError::SourceSlice {
                    source: input.ordinal,
                    start: occurrence.binding_span.start,
                    end: occurrence.binding_span.end,
                })?;
            let name = context.assembly.intern(name_text)?;
            context.current_group = bound
                .type_group
                .and_then(|group| context.group_keys.get(&group).copied())
                .or_else(|| extra_group_by_declaration.get(&bound.id.0).copied());
            let child_region = child_regions.get(&occurrence.binding_span.start).copied();
            let region = region_ranges
                .iter()
                .filter(|(start, end, header, _)| {
                    *header != occurrence.binding_span.start
                        && *start <= occurrence.declaration_span.start
                        && occurrence.declaration_span.end <= *end
                })
                .min_by_key(|(start, end, _, _)| end.saturating_sub(*start))
                .map(|(_, _, _, key)| *key)
                .unwrap_or(*root_region);
            context.current_scope = bound.site.scope;
            context.current_region = region;
            let lexical_scope = match bound.site.scope {
                None => product::DeclarationLexicalScope::None,
                Some(scope) => context
                    .callable_scope_keys
                    .get(&scope.0)
                    .copied()
                    .map(product::DeclarationLexicalScope::Callable)
                    .or_else(|| {
                        lexical_regions
                            .get(&scope.0)
                            .copied()
                            .map(product::DeclarationLexicalScope::Region)
                    })
                    .ok_or(LibraryProductProjectionError::MissingAuthenticatedScope {
                        source: input.ordinal,
                        binding_start: occurrence.binding_span.start,
                    })?,
            };
            let payload = lower_declaration_payload(
                &mut context,
                occurrence.product_payload,
                occurrence.kind,
                child_region,
            )?;
            let declaration = context
                .assembly
                .add_declaration(product::DeclarationRecipe {
                    site: context.site(occurrence.declaration_span),
                    binding_site: context.site(occurrence.binding_span),
                    region,
                    lexical_scope,
                    name,
                    spaces: product::DeclarationSlots {
                        value: bound.value_storage.is_some(),
                        r#type: bound.type_group.is_some(),
                        namespace: bound.namespace.is_some(),
                    },
                    flags: product::DeclarationFlags {
                        ambient: true,
                        constant: occurrence.variable_constant,
                        r#abstract: matches!(
                            occurrence.product_payload,
                            Some(SourceDeclarationProductPayload::Class(class)) if class.r#abstract
                        ),
                        ..product::DeclarationFlags::default()
                    },
                    payload,
                })?;
            declaration_keys.insert(bound.id.0, declaration);
            declaration_facts.push(DeclarationFact {
                key: declaration,
                container: context.region_containers[&region],
                value: bound.value_storage.is_some(),
                namespace: bound.namespace.is_some(),
                type_group: bound.type_group,
                group: context.current_group,
                stable: StableDeclarationAnchor {
                    site: StableSourceSite {
                        source_ordinal: input.ordinal,
                        start: occurrence.declaration_span.start,
                        end: occurrence.declaration_span.end,
                    },
                    binding_start: occurrence.binding_span.start,
                    binding_end: occurrence.binding_span.end,
                    name: name_text.to_owned(),
                },
            });
            context
                .assembly
                .add_lexical_step(product::LexicalStepRecipe {
                    owner: declaration,
                    kind: product::LexicalStepKind::Declaration,
                    site: context.site(occurrence.declaration_span),
                });
            if occurrence.kind == DeclarationKind::Interface {
                let fragment = input
                    .manifest
                    .type_fragments()
                    .iter()
                    .find(|fragment| fragment.binding_span == occurrence.binding_span)
                    .ok_or(
                        LibraryProductProjectionError::MissingAuthenticatedDeclaration {
                            source: input.ordinal,
                            binding_start: occurrence.binding_span.start,
                        },
                    )?;
                for lexical in input
                    .manifest
                    .interface_occurrences(fragment.interface_occurrences.clone())
                {
                    let kind = match lexical.kind {
                        SourceInterfaceOccurrenceKind::Header => continue,
                        SourceInterfaceOccurrenceKind::Heritage => {
                            product::LexicalStepKind::Heritage
                        }
                        SourceInterfaceOccurrenceKind::Member => product::LexicalStepKind::Member,
                    };
                    context
                        .assembly
                        .add_lexical_step(product::LexicalStepRecipe {
                            owner: declaration,
                            kind,
                            site: context.site(lexical.span),
                        });
                }
            }
        }
    }
    for group in binder.type_groups.iter().filter(|group| {
        group.fragments.iter().any(|fragment| {
            sources
                .iter()
                .any(|source| source.module == fragment.site.module)
        })
    }) {
        let key = context.group_keys[&group.id];
        let fragments = group
            .fragments
            .iter()
            .map(|fragment| {
                declaration_keys
                    .get(&fragment.declaration.0)
                    .copied()
                    .ok_or(
                        LibraryProductProjectionError::MissingAuthenticatedDeclaration {
                            source: fragment.source.0,
                            binding_start: fragment.site.binding_span.start,
                        },
                    )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let first = group.fragments.first().ok_or(
            LibraryProductProjectionError::MissingAuthenticatedDeclaration {
                source: 0,
                binding_start: 0,
            },
        )?;
        let first_declaration = declaration_keys.get(&first.declaration.0).copied().ok_or(
            LibraryProductProjectionError::MissingAuthenticatedDeclaration {
                source: first.source.0,
                binding_start: first.site.binding_span.start,
            },
        )?;
        let container = declaration_facts
            .iter()
            .find(|fact| fact.key == first_declaration)
            .map(|fact| fact.container)
            .ok_or(
                LibraryProductProjectionError::MissingAuthenticatedDeclaration {
                    source: first.source.0,
                    binding_start: first.site.binding_span.start,
                },
            )?;
        let mut slots = product::DeclarationSlots {
            value: false,
            r#type: true,
            namespace: false,
        };
        for fragment in &group.fragments {
            if let Some(declaration) = binder.declarations.get(fragment.declaration) {
                slots.value |= declaration.value_storage.is_some();
                slots.namespace |= declaration.namespace.is_some();
                let product_declaration = declaration_keys[&fragment.declaration.0];
                if declaration.value_storage.is_some() {
                    context.add_value_attachment(product::ValueAttachmentRecipe {
                        group: key,
                        declaration: product_declaration,
                    })?;
                }
                if declaration.namespace.is_some() {
                    context.add_namespace_attachment(product::NamespaceAttachmentRecipe {
                        group: key,
                        declaration: product_declaration,
                    })?;
                }
            }
        }
        context.assembly.set_group_shape(
            key,
            container,
            slots,
            fragments.clone().into_boxed_slice(),
        )?;
        let anchor = stable_group_anchor_from_facts(
            &context,
            container,
            slots,
            &fragments,
            &declaration_facts,
        )?;
        context.stable_group_anchors.insert(key, anchor);
    }
    let mut extra_groups = BTreeMap::<product::GroupKey, Vec<&DeclarationFact>>::new();
    for fact in &declaration_facts {
        if fact.type_group.is_none() && (fact.value || fact.namespace) {
            if let Some(group) = fact.group {
                extra_groups.entry(group).or_default().push(fact);
            }
        }
    }
    for (key, mut facts) in extra_groups {
        facts.sort_by_key(|fact| fact.key);
        let fragment_keys = facts.iter().map(|fact| fact.key).collect::<Vec<_>>();
        let slots = product::DeclarationSlots {
            value: facts.iter().any(|fact| fact.value),
            r#type: false,
            namespace: facts.iter().any(|fact| fact.namespace),
        };
        context.assembly.set_group_shape(
            key,
            facts[0].container,
            slots,
            fragment_keys.clone().into_boxed_slice(),
        )?;
        let anchor = stable_group_anchor_from_facts(
            &context,
            facts[0].container,
            slots,
            &fragment_keys,
            &declaration_facts,
        )?;
        context.stable_group_anchors.insert(key, anchor);
        for fact in facts {
            if fact.value {
                context.add_value_attachment(product::ValueAttachmentRecipe {
                    group: key,
                    declaration: fact.key,
                })?;
            }
            if fact.namespace {
                context.add_namespace_attachment(product::NamespaceAttachmentRecipe {
                    group: key,
                    declaration: fact.key,
                })?;
            }
        }
    }
    let pending_dependencies = std::mem::take(&mut context.pending_dependencies);
    let mut dependency_oracle = pending_dependencies
        .iter()
        .map(|pending| stable_dependency_from_binder(&context, pending))
        .collect::<Result<Vec<_>, _>>()?;
    dependency_oracle.sort();
    let mut namespace_oracle = declaration_facts
        .iter()
        .filter(|fact| fact.namespace)
        .map(|fact| stable_attachment_from_binder(&context, fact, "namespace"))
        .collect::<Result<Vec<_>, _>>()?;
    namespace_oracle.sort();
    let mut value_oracle = declaration_facts
        .iter()
        .filter(|fact| fact.value)
        .map(|fact| stable_attachment_from_binder(&context, fact, "value"))
        .collect::<Result<Vec<_>, _>>()?;
    value_oracle.sort();
    let mut container_oracle = context
        .stable_container_anchors
        .values()
        .cloned()
        .collect::<Vec<_>>();
    container_oracle.sort();
    for pending in pending_dependencies {
        let target = match pending.target {
            PendingDependencyTarget::Group(group) => product::DependencyTarget::Group(group),
            PendingDependencyTarget::TypeParameter(parameter) => {
                product::DependencyTarget::TypeParameter(parameter)
            }
            PendingDependencyTarget::IntrinsicGlobalValue => {
                product::DependencyTarget::IntrinsicGlobalValue
            }
            PendingDependencyTarget::Unresolved => product::DependencyTarget::Unresolved {
                region: pending.region,
                path: pending.path_keys.into_boxed_slice(),
            },
        };
        let row = product::DependencyRecipe {
            owner: pending.owner,
            target,
            kind: pending.kind,
            site: pending.site,
        };
        context.assembly.add_dependency(row);
    }
    let product = context
        .assembly
        .finish(profile_identity)
        .map_err(LibraryProductProjectionError::Product)?;
    let mut product_dependencies = stable_dependencies_from_product(&product)?;
    product_dependencies.sort();
    if product_dependencies != dependency_oracle {
        return Err(LibraryProductProjectionError::AuthenticatedOracleMismatch {
            table: "dependencies",
        });
    }
    let mut product_containers = stable_containers_from_product(&product)?;
    product_containers.sort();
    if product_containers != container_oracle {
        return Err(LibraryProductProjectionError::AuthenticatedOracleMismatch {
            table: "containers",
        });
    }
    let mut product_namespaces =
        stable_attachments_from_product(&product, product.namespace_attachment_rows())?;
    product_namespaces.sort();
    if product_namespaces != namespace_oracle {
        return Err(LibraryProductProjectionError::AuthenticatedOracleMismatch {
            table: "namespaces",
        });
    }
    let mut product_values =
        stable_attachments_from_product(&product, product.value_attachment_rows())?;
    product_values.sort();
    if product_values != value_oracle {
        return Err(LibraryProductProjectionError::AuthenticatedOracleMismatch { table: "values" });
    }
    Ok(product)
}

struct ProjectionContext<'binder> {
    binder: &'binder Binder,
    assembly: product::ProductAssembly,
    source: product::SourceKey,
    source_ordinal: u32,
    group_keys: BTreeMap<TypeGroupId, product::GroupKey>,
    value_group_keys: BTreeMap<u32, product::GroupKey>,
    type_parameters: BTreeMap<(u32, u32), product::TypeParameterKey>,
    type_by_site: BTreeMap<(u32, u32, u32), product::TypeRecipeKey>,
    pending_dependencies: Vec<PendingDependency>,
    current_group: Option<product::GroupKey>,
    current_scope: Option<ScopeId>,
    current_region: product::RegionKey,
    callable_scope_keys: BTreeMap<u32, product::CallableScopeKey>,
    active_type_parameters: Vec<ActiveTypeParameterFrame>,
    stable_group_anchors: BTreeMap<product::GroupKey, StableGroupAnchor>,
    stable_type_parameter_anchors: BTreeMap<product::TypeParameterKey, StableTypeParameterAnchor>,
    stable_region_anchors: BTreeMap<product::RegionKey, StableRegionAnchor>,
    stable_container_anchors: BTreeMap<product::ContainerKey, StableContainerAnchor>,
    global_container: product::ContainerKey,
    namespace_containers: BTreeMap<NamespaceId, product::ContainerKey>,
    region_containers: BTreeMap<product::RegionKey, product::ContainerKey>,
}

#[derive(Clone)]
struct NamespaceContainerFact {
    anchor: StableSourceSite,
    ambient_external: bool,
}

fn container_for_namespace_owner(
    binder: &Binder,
    owner: NamespaceOwner,
    global: product::ContainerKey,
    scopes: &BTreeMap<u32, product::ContainerKey>,
    namespaces: &BTreeMap<NamespaceId, product::ContainerKey>,
) -> Result<product::ContainerKey, LibraryProductProjectionError> {
    match owner {
        NamespaceOwner::CompilationGlobal => Ok(global),
        NamespaceOwner::NamespacePublic(namespace) => namespaces.get(&namespace).copied().ok_or(
            LibraryProductProjectionError::MissingAuthenticatedScope {
                source: 0,
                binding_start: 0,
            },
        ),
        NamespaceOwner::FragmentPrivate(fragment) => {
            let namespace = binder.namespaces.fragment(fragment).ok_or(
                LibraryProductProjectionError::MissingAuthenticatedScope {
                    source: 0,
                    binding_start: 0,
                },
            )?;
            namespaces.get(&namespace.namespace).copied().ok_or(
                LibraryProductProjectionError::MissingAuthenticatedScope {
                    source: 0,
                    binding_start: namespace.source_start,
                },
            )
        }
        NamespaceOwner::Lexical(scope) => container_for_scope(binder, scope, global, scopes).ok_or(
            LibraryProductProjectionError::MissingAuthenticatedScope {
                source: 0,
                binding_start: 0,
            },
        ),
    }
}

fn container_for_scope(
    binder: &Binder,
    mut scope: ScopeId,
    global: product::ContainerKey,
    containers: &BTreeMap<u32, product::ContainerKey>,
) -> Option<product::ContainerKey> {
    loop {
        if let Some(container) = containers.get(&scope.0) {
            return Some(*container);
        }
        let record = binder.graph.get(scope)?;
        match record.kind {
            ScopeKind::CompilationGlobal
            | ScopeKind::ScriptNamespaceRoot
            | ScopeKind::GlobalOverlay => return Some(global),
            ScopeKind::NamespacePrivate => {
                if let Some(public) = record.namespace_public {
                    if let Some(container) = containers.get(&public.0) {
                        return Some(*container);
                    }
                }
            }
            ScopeKind::NamespacePublic
            | ScopeKind::Module
            | ScopeKind::Function
            | ScopeKind::Block => {}
        }
        scope = record.parent?;
    }
}

fn insert_lexical_region(
    regions: &mut BTreeMap<u32, product::RegionKey>,
    scope: ScopeId,
    region: product::RegionKey,
    source: u32,
) -> Result<(), LibraryProductProjectionError> {
    if regions
        .insert(scope.0, region)
        .is_some_and(|existing| existing != region)
    {
        return Err(LibraryProductProjectionError::AmbiguousAuthenticatedScope {
            source,
            scope: scope.0,
        });
    }
    Ok(())
}

struct PendingDependency {
    owner: product::GroupKey,
    source_ordinal: u32,
    scope: ScopeId,
    region: product::RegionKey,
    path: Vec<String>,
    path_keys: Vec<product::StringKey>,
    target: PendingDependencyTarget,
    space: DependencySpace,
    kind: product::DependencyKind,
    site: product::SourceSite,
}

#[derive(Clone, Copy)]
enum PendingDependencyTarget {
    Group(product::GroupKey),
    TypeParameter(product::TypeParameterKey),
    IntrinsicGlobalValue,
    Unresolved,
}

#[derive(Clone, Copy)]
enum DependencySpace {
    Type,
    Value,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TypeParameterFrameKind {
    Lexical,
    ConditionalInfer,
}

struct ActiveTypeParameterFrame {
    kind: TypeParameterFrameKind,
    bindings: BTreeMap<String, product::TypeParameterKey>,
    owner: product::SourceSite,
    visibility: Box<[product::SourceSite]>,
}
struct DeclarationFact {
    key: product::DeclarationKey,
    container: product::ContainerKey,
    value: bool,
    namespace: bool,
    type_group: Option<TypeGroupId>,
    group: Option<product::GroupKey>,
    stable: StableDeclarationAnchor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct StableSourceSite {
    source_ordinal: u32,
    start: u32,
    end: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct StableRegionAnchor {
    site: StableSourceSite,
    kind: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct StableContainerSegment {
    kind: &'static str,
    name: Option<String>,
    anchor: Option<StableSourceSite>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct StableContainerAnchor {
    path: Vec<StableContainerSegment>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct StableDeclarationAnchor {
    site: StableSourceSite,
    binding_start: u32,
    binding_end: u32,
    name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct StableGroupAnchor {
    source_ordinal: u32,
    binding_start: u32,
    binding_end: u32,
    name: String,
    container: StableContainerAnchor,
    slots: StableDeclarationSlots,
    fragments: Vec<StableDeclarationAnchor>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct StableDeclarationSlots {
    value: bool,
    r#type: bool,
    namespace: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct StableTypeParameterAnchor {
    site: StableSourceSite,
    owner: StableSourceSite,
    name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum StableDependencyTarget {
    Group(StableGroupAnchor),
    TypeParameter(StableTypeParameterAnchor),
    IntrinsicGlobalValue,
    Unresolved {
        region: StableRegionAnchor,
        path: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct StableDependencyEvidence {
    owner: StableGroupAnchor,
    target: StableDependencyTarget,
    kind: &'static str,
    site: StableSourceSite,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct StableAttachmentEvidence {
    group: StableGroupAnchor,
    declaration: StableDeclarationAnchor,
    slot: &'static str,
    container: StableContainerAnchor,
    declaration_spaces: StableDeclarationSlots,
}

fn stable_container_from_parts(
    key: product::ContainerKey,
    parents: &BTreeMap<product::ContainerKey, Option<product::ContainerKey>>,
    segments: &BTreeMap<product::ContainerKey, StableContainerSegment>,
) -> Result<StableContainerAnchor, LibraryProductProjectionError> {
    let mut reverse = Vec::new();
    let mut current = Some(key);
    while let Some(container) = current {
        if reverse.len() > segments.len() {
            return Err(LibraryProductProjectionError::AuthenticatedOracleMismatch {
                table: "container-cycle",
            });
        }
        reverse.push(segments.get(&container).cloned().ok_or(
            LibraryProductProjectionError::AuthenticatedOracleMismatch {
                table: "container-segment",
            },
        )?);
        current = parents.get(&container).copied().flatten();
    }
    reverse.reverse();
    Ok(StableContainerAnchor { path: reverse })
}

fn stable_slots(slots: product::DeclarationSlots) -> StableDeclarationSlots {
    StableDeclarationSlots {
        value: slots.value,
        r#type: slots.r#type,
        namespace: slots.namespace,
    }
}

fn stable_group_anchor_from_facts(
    context: &ProjectionContext<'_>,
    container: product::ContainerKey,
    slots: product::DeclarationSlots,
    fragments: &[product::DeclarationKey],
    facts: &[DeclarationFact],
) -> Result<StableGroupAnchor, LibraryProductProjectionError> {
    let stable_fragments = fragments
        .iter()
        .map(|fragment| {
            facts
                .iter()
                .find(|fact| fact.key == *fragment)
                .map(|fact| fact.stable.clone())
                .ok_or(LibraryProductProjectionError::AuthenticatedOracleMismatch {
                    table: "group-fragment",
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let first = stable_fragments.first().cloned().ok_or(
        LibraryProductProjectionError::AuthenticatedOracleMismatch {
            table: "group-first-fragment",
        },
    )?;
    Ok(StableGroupAnchor {
        source_ordinal: first.site.source_ordinal,
        binding_start: first.binding_start,
        binding_end: first.binding_end,
        name: first.name.clone(),
        container: context
            .stable_container_anchors
            .get(&container)
            .cloned()
            .ok_or(LibraryProductProjectionError::AuthenticatedOracleMismatch {
                table: "group-container",
            })?,
        slots: stable_slots(slots),
        fragments: stable_fragments,
    })
}

impl ProjectionContext<'_> {
    fn add_namespace_attachment(
        &mut self,
        row: product::NamespaceAttachmentRecipe,
    ) -> Result<(), LibraryProductProjectionError> {
        self.assembly.add_namespace(row)?;
        Ok(())
    }

    fn add_value_attachment(
        &mut self,
        row: product::ValueAttachmentRecipe,
    ) -> Result<(), LibraryProductProjectionError> {
        self.assembly.add_value(row)?;
        Ok(())
    }

    fn site(&self, span: Span) -> product::SourceSite {
        product::SourceSite {
            source: self.source,
            start: span.start,
            end: span.end,
        }
    }
    fn oxc_site(&self, span: oxc_span::Span) -> product::SourceSite {
        self.site(Span::from_oxc(span))
    }
    fn unsupported(
        &self,
        span: oxc_span::Span,
        form: &'static str,
    ) -> LibraryProductProjectionError {
        LibraryProductProjectionError::UnsupportedType {
            source: self.source_ordinal,
            start: span.start,
            form,
        }
    }
}

fn stable_dependency_from_binder(
    context: &ProjectionContext<'_>,
    pending: &PendingDependency,
) -> Result<StableDependencyEvidence, LibraryProductProjectionError> {
    let owner = context
        .stable_group_anchors
        .get(&pending.owner)
        .cloned()
        .ok_or(LibraryProductProjectionError::AuthenticatedOracleMismatch {
            table: "dependency-owner",
        })?;
    let target = match pending.target {
        PendingDependencyTarget::TypeParameter(parameter) => StableDependencyTarget::TypeParameter(
            context
                .stable_type_parameter_anchors
                .get(&parameter)
                .cloned()
                .ok_or(LibraryProductProjectionError::AuthenticatedOracleMismatch {
                    table: "dependency-type-parameter",
                })?,
        ),
        PendingDependencyTarget::IntrinsicGlobalValue => {
            StableDependencyTarget::IntrinsicGlobalValue
        }
        PendingDependencyTarget::Group(_) | PendingDependencyTarget::Unresolved => {
            let resolved_group = match pending.space {
                DependencySpace::Type => {
                    resolved_type_group(context.binder, pending.scope, &pending.path)
                        .and_then(|group| context.group_keys.get(&group).copied())
                }
                DependencySpace::Value => {
                    resolved_value_storage(context.binder, pending.scope, &pending.path)
                        .and_then(|storage| context.value_group_keys.get(&storage.0).copied())
                }
            };
            if let Some(group) = resolved_group {
                StableDependencyTarget::Group(
                    context.stable_group_anchors.get(&group).cloned().ok_or(
                        LibraryProductProjectionError::AuthenticatedOracleMismatch {
                            table: "dependency-target",
                        },
                    )?,
                )
            } else {
                StableDependencyTarget::Unresolved {
                    region: context
                        .stable_region_anchors
                        .get(&pending.region)
                        .cloned()
                        .ok_or(LibraryProductProjectionError::AuthenticatedOracleMismatch {
                            table: "dependency-region",
                        })?,
                    path: pending.path.clone(),
                }
            }
        }
    };
    Ok(StableDependencyEvidence {
        owner,
        target,
        kind: dependency_kind_name(pending.kind),
        site: StableSourceSite {
            source_ordinal: pending.source_ordinal,
            start: pending.site.start,
            end: pending.site.end,
        },
    })
}

fn stable_attachment_from_binder(
    context: &ProjectionContext<'_>,
    fact: &DeclarationFact,
    slot: &'static str,
) -> Result<StableAttachmentEvidence, LibraryProductProjectionError> {
    let group = fact
        .group
        .and_then(|group| context.stable_group_anchors.get(&group))
        .cloned()
        .ok_or(LibraryProductProjectionError::AuthenticatedOracleMismatch {
            table: "attachment-group",
        })?;
    Ok(StableAttachmentEvidence {
        container: group.container.clone(),
        group,
        declaration: fact.stable.clone(),
        slot,
        declaration_spaces: StableDeclarationSlots {
            value: fact.value,
            r#type: fact.type_group.is_some(),
            namespace: fact.namespace,
        },
    })
}

fn stable_dependencies_from_product(
    declaration_product: &product::LibraryDeclarationProduct,
) -> Result<Vec<StableDependencyEvidence>, LibraryProductProjectionError> {
    declaration_product
        .dependency_rows()
        .iter()
        .map(|row| {
            let target = match &row.target {
                product::DependencyTarget::Group(group) => StableDependencyTarget::Group(
                    stable_group_from_product(declaration_product, *group)?,
                ),
                product::DependencyTarget::TypeParameter(parameter) => {
                    StableDependencyTarget::TypeParameter(stable_type_parameter_from_product(
                        declaration_product,
                        *parameter,
                    )?)
                }
                product::DependencyTarget::IntrinsicGlobalValue => {
                    StableDependencyTarget::IntrinsicGlobalValue
                }
                product::DependencyTarget::Unresolved { region, path } => {
                    StableDependencyTarget::Unresolved {
                        region: stable_region_from_product(declaration_product, *region)?,
                        path: path
                            .iter()
                            .map(|key| stable_string(declaration_product, *key))
                            .collect::<Result<Vec<_>, _>>()?,
                    }
                }
            };
            Ok(StableDependencyEvidence {
                owner: stable_group_from_product(declaration_product, row.owner)?,
                target,
                kind: dependency_kind_name(row.kind),
                site: stable_site_from_product(declaration_product, row.site)?,
            })
        })
        .collect()
}

fn stable_attachments_from_product<T>(
    declaration_product: &product::LibraryDeclarationProduct,
    rows: &[T],
) -> Result<Vec<StableAttachmentEvidence>, LibraryProductProjectionError>
where
    T: ProductAttachmentRow,
{
    rows.iter()
        .map(|row| {
            Ok(StableAttachmentEvidence {
                group: stable_group_from_product(declaration_product, row.group())?,
                declaration: stable_declaration_from_product(
                    declaration_product,
                    row.declaration(),
                )?,
                slot: row.slot(),
                container: stable_container_from_product(
                    declaration_product,
                    declaration_product.groups()[row.group().index()]
                        .container
                        .index(),
                )?,
                declaration_spaces: stable_slots(
                    declaration_product.declarations()[row.declaration().index()].spaces,
                ),
            })
        })
        .collect()
}

trait ProductAttachmentRow {
    fn group(&self) -> product::GroupKey;
    fn declaration(&self) -> product::DeclarationKey;
    fn slot(&self) -> &'static str;
}

impl ProductAttachmentRow for product::NamespaceAttachmentRecipe {
    fn group(&self) -> product::GroupKey {
        self.group
    }
    fn declaration(&self) -> product::DeclarationKey {
        self.declaration
    }
    fn slot(&self) -> &'static str {
        "namespace"
    }
}

impl ProductAttachmentRow for product::ValueAttachmentRecipe {
    fn group(&self) -> product::GroupKey {
        self.group
    }
    fn declaration(&self) -> product::DeclarationKey {
        self.declaration
    }
    fn slot(&self) -> &'static str {
        "value"
    }
}

fn stable_group_from_product(
    declaration_product: &product::LibraryDeclarationProduct,
    key: product::GroupKey,
) -> Result<StableGroupAnchor, LibraryProductProjectionError> {
    let group = declaration_product.groups().get(key.index()).ok_or(
        LibraryProductProjectionError::AuthenticatedOracleMismatch {
            table: "product-group",
        },
    )?;
    let declaration = group.fragments.first().copied().ok_or(
        LibraryProductProjectionError::AuthenticatedOracleMismatch {
            table: "product-group-fragment",
        },
    )?;
    let declaration = stable_declaration_from_product(declaration_product, declaration)?;
    let fragments = group
        .fragments
        .iter()
        .map(|fragment| stable_declaration_from_product(declaration_product, *fragment))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(StableGroupAnchor {
        source_ordinal: declaration.site.source_ordinal,
        binding_start: declaration.binding_start,
        binding_end: declaration.binding_end,
        name: stable_string(declaration_product, group.name)?,
        container: stable_container_from_product(declaration_product, group.container.index())?,
        slots: stable_slots(group.slots),
        fragments,
    })
}

fn stable_containers_from_product(
    declaration_product: &product::LibraryDeclarationProduct,
) -> Result<Vec<StableContainerAnchor>, LibraryProductProjectionError> {
    declaration_product
        .containers()
        .iter()
        .enumerate()
        .map(|(row, _)| stable_container_from_product(declaration_product, row))
        .collect()
}

fn stable_container_from_product(
    declaration_product: &product::LibraryDeclarationProduct,
    key: usize,
) -> Result<StableContainerAnchor, LibraryProductProjectionError> {
    let mut reverse = Vec::new();
    let mut current = Some(key);
    while let Some(container_index) = current {
        if reverse.len() > declaration_product.containers().len() {
            return Err(LibraryProductProjectionError::AuthenticatedOracleMismatch {
                table: "product-container-cycle",
            });
        }
        let container = declaration_product
            .containers()
            .get(container_index)
            .ok_or(LibraryProductProjectionError::AuthenticatedOracleMismatch {
                table: "product-container",
            })?;
        reverse.push(StableContainerSegment {
            kind: container_kind_name(container.kind),
            name: container
                .name
                .map(|name| stable_string(declaration_product, name))
                .transpose()?,
            anchor: container
                .anchor
                .map(|site| stable_site_from_product(declaration_product, site))
                .transpose()?,
        });
        current = container.parent.map(product::ContainerKey::index);
    }
    reverse.reverse();
    Ok(StableContainerAnchor { path: reverse })
}

fn stable_declaration_from_product(
    declaration_product: &product::LibraryDeclarationProduct,
    key: product::DeclarationKey,
) -> Result<StableDeclarationAnchor, LibraryProductProjectionError> {
    let declaration = declaration_product.declarations().get(key.index()).ok_or(
        LibraryProductProjectionError::AuthenticatedOracleMismatch {
            table: "product-declaration",
        },
    )?;
    Ok(StableDeclarationAnchor {
        site: stable_site_from_product(declaration_product, declaration.site)?,
        binding_start: declaration.binding_site.start,
        binding_end: declaration.binding_site.end,
        name: stable_string(declaration_product, declaration.name)?,
    })
}

fn stable_type_parameter_from_product(
    declaration_product: &product::LibraryDeclarationProduct,
    key: product::TypeParameterKey,
) -> Result<StableTypeParameterAnchor, LibraryProductProjectionError> {
    let parameter = declaration_product
        .type_parameters()
        .get(key.index())
        .ok_or(LibraryProductProjectionError::AuthenticatedOracleMismatch {
            table: "product-type-parameter",
        })?;
    Ok(StableTypeParameterAnchor {
        site: stable_site_from_product(declaration_product, parameter.site)?,
        owner: stable_site_from_product(declaration_product, parameter.owner)?,
        name: stable_string(declaration_product, parameter.name)?,
    })
}

fn stable_region_from_product(
    declaration_product: &product::LibraryDeclarationProduct,
    key: product::RegionKey,
) -> Result<StableRegionAnchor, LibraryProductProjectionError> {
    let region = declaration_product.regions().get(key.index()).ok_or(
        LibraryProductProjectionError::AuthenticatedOracleMismatch {
            table: "product-region",
        },
    )?;
    Ok(StableRegionAnchor {
        site: stable_site_from_product(declaration_product, region.site)?,
        kind: region_kind_name(region.kind),
    })
}

fn stable_site_from_product(
    declaration_product: &product::LibraryDeclarationProduct,
    site: product::SourceSite,
) -> Result<StableSourceSite, LibraryProductProjectionError> {
    let source = declaration_product
        .sources()
        .get(site.source.index())
        .ok_or(LibraryProductProjectionError::AuthenticatedOracleMismatch {
            table: "product-source",
        })?;
    Ok(StableSourceSite {
        source_ordinal: source.ordinal,
        start: site.start,
        end: site.end,
    })
}

fn stable_string(
    declaration_product: &product::LibraryDeclarationProduct,
    key: product::StringKey,
) -> Result<String, LibraryProductProjectionError> {
    declaration_product
        .strings()
        .get(key.index())
        .cloned()
        .ok_or(LibraryProductProjectionError::AuthenticatedOracleMismatch {
            table: "product-string",
        })
}

fn region_kind_name(kind: product::RegionKind) -> &'static str {
    match kind {
        product::RegionKind::Source => "source",
        product::RegionKind::Namespace => "namespace",
        product::RegionKind::GlobalAugmentation => "global-augmentation",
        product::RegionKind::AmbientExternalModule => "ambient-external-module",
    }
}

fn container_kind_name(kind: product::ContainerKind) -> &'static str {
    match kind {
        product::ContainerKind::CompilationGlobal => "compilation-global",
        product::ContainerKind::ExternalModule => "external-module",
        product::ContainerKind::Namespace => "namespace",
        product::ContainerKind::AmbientExternalModule => "ambient-external-module",
    }
}

fn dependency_kind_name(kind: product::DependencyKind) -> &'static str {
    match kind {
        product::DependencyKind::Reference => "reference",
        product::DependencyKind::Heritage => "heritage",
        product::DependencyKind::Query => "query",
        product::DependencyKind::Import => "import",
        product::DependencyKind::IndexedAccess => "indexed-access",
        product::DependencyKind::Constraint => "constraint",
    }
}

fn lower_declaration_payload(
    context: &mut ProjectionContext<'_>,
    payload: Option<SourceDeclarationProductPayload<'_>>,
    kind: DeclarationKind,
    child_region: Option<product::RegionKey>,
) -> Result<product::DeclarationPayload, LibraryProductProjectionError> {
    let annotation = |context: &mut ProjectionContext<'_>,
                      annotation: Option<&oxc_ast::ast::TSTypeAnnotation<'_>>|
     -> Result<Option<product::TypeRecipeKey>, LibraryProductProjectionError> {
        annotation
            .map(|value| lower_type(context, &value.type_annotation))
            .transpose()
    };
    match (kind, payload) {
        (DeclarationKind::Variable, Some(SourceDeclarationProductPayload::Variable(value))) => {
            Ok(product::DeclarationPayload::Variable {
                annotation: annotation(context, value.type_annotation.as_deref())?,
            })
        }
        (DeclarationKind::Parameter, Some(SourceDeclarationProductPayload::Parameter(value))) => {
            Ok(product::DeclarationPayload::Parameter {
                annotation: annotation(context, value.type_annotation.as_deref())?,
            })
        }
        (
            DeclarationKind::Parameter,
            Some(SourceDeclarationProductPayload::RestParameter(value)),
        ) => Ok(product::DeclarationPayload::Parameter {
            annotation: annotation(context, value.type_annotation.as_deref())?,
        }),
        (DeclarationKind::CatchParameter, Some(SourceDeclarationProductPayload::Catch(value))) => {
            Ok(product::DeclarationPayload::CatchParameter {
                annotation: annotation(context, value.type_annotation.as_deref())?,
            })
        }
        (DeclarationKind::Function, Some(SourceDeclarationProductPayload::Function(value))) => {
            Ok(product::DeclarationPayload::Function {
                signature: lower_signature(
                    context,
                    value.type_parameters.as_deref(),
                    value.this_param.as_deref(),
                    &value.params,
                    value.return_type.as_deref(),
                    value.span,
                )?,
            })
        }
        (DeclarationKind::TypeAlias, Some(SourceDeclarationProductPayload::TypeAlias(value))) => {
            let type_parameters = lower_declaration_type_parameters(
                context,
                value.type_parameters.as_deref(),
                value.span,
            )?;
            let result = Ok(product::DeclarationPayload::TypeAlias {
                type_parameters,
                value: lower_type(context, &value.type_annotation)?,
            });
            context.active_type_parameters.pop();
            result
        }
        (DeclarationKind::Interface, Some(SourceDeclarationProductPayload::Interface(value))) => {
            let type_parameters = lower_declaration_type_parameters(
                context,
                value.type_parameters.as_deref(),
                value.span,
            )?;
            let mut heritage = Vec::with_capacity(value.extends.len());
            for heritage_item in &value.extends {
                let node = lower_heritage(
                    context,
                    &heritage_item.expression,
                    heritage_item.type_arguments.as_deref(),
                    heritage_item.span,
                )?;
                heritage.push(context.assembly.add_type_recipe(product::TypeRecipe {
                    site: context.oxc_site(heritage_item.span),
                    node,
                })?);
            }
            let mut members = Vec::with_capacity(value.body.body.len());
            for member in &value.body.body {
                members.push(lower_member(context, member)?);
            }
            let result = Ok(product::DeclarationPayload::Interface {
                type_parameters,
                members: members.into_boxed_slice(),
                heritage: heritage.into_boxed_slice(),
            });
            context.active_type_parameters.pop();
            result
        }
        (DeclarationKind::Class, Some(SourceDeclarationProductPayload::Class(value))) => {
            lower_class(context, value)
        }
        (DeclarationKind::Enum, Some(SourceDeclarationProductPayload::Enum(value))) => {
            Err(context.unsupported(value.span, "enum-declaration"))
        }
        (DeclarationKind::Namespace, Some(SourceDeclarationProductPayload::Namespace(_))) => {
            Ok(product::DeclarationPayload::Namespace {
                child_region: child_region.ok_or(
                    LibraryProductProjectionError::MissingPayload {
                        source: context.source_ordinal,
                        binding_start: 0,
                    },
                )?,
            })
        }
        (DeclarationKind::Global, Some(SourceDeclarationProductPayload::Global(_))) => {
            Ok(product::DeclarationPayload::Global {
                child_region: child_region.ok_or(
                    LibraryProductProjectionError::MissingPayload {
                        source: context.source_ordinal,
                        binding_start: 0,
                    },
                )?,
            })
        }
        (DeclarationKind::Import, _) => Ok(product::DeclarationPayload::Import),
        (DeclarationKind::ImportEquals, _) => Ok(product::DeclarationPayload::ImportEquals),
        (DeclarationKind::NamespaceExport, _) => Ok(product::DeclarationPayload::NamespaceExport),
        (DeclarationKind::DefaultExportExpression, _) => {
            Ok(product::DeclarationPayload::DefaultExportExpression)
        }
        (_, None) => Err(LibraryProductProjectionError::MissingPayload {
            source: context.source_ordinal,
            binding_start: 0,
        }),
        _ => Err(LibraryProductProjectionError::MissingPayload {
            source: context.source_ordinal,
            binding_start: 0,
        }),
    }
}

fn lower_class(
    context: &mut ProjectionContext<'_>,
    class: &oxc_ast::ast::Class<'_>,
) -> Result<product::DeclarationPayload, LibraryProductProjectionError> {
    let type_parameters =
        lower_declaration_type_parameters(context, class.type_parameters.as_deref(), class.span)?;
    let mut heritage = Vec::new();
    if let Some(super_class) = &class.super_class {
        let node = lower_heritage(
            context,
            super_class,
            class.super_type_arguments.as_deref(),
            super_class.span(),
        )?;
        heritage.push(context.assembly.add_type_recipe(product::TypeRecipe {
            site: context.oxc_site(super_class.span()),
            node,
        })?);
    }
    for implementation in &class.implements {
        let node = lower_reference_parts(
            context,
            &implementation.expression,
            implementation.type_arguments.as_deref(),
            implementation.span,
        )?;
        heritage.push(context.assembly.add_type_recipe(product::TypeRecipe {
            site: context.oxc_site(implementation.span),
            node,
        })?);
    }
    let mut members = Vec::with_capacity(class.body.body.len());
    for element in &class.body.body {
        match element {
            oxc_ast::ast::ClassElement::MethodDefinition(item) => {
                let kind = match item.kind {
                    oxc_ast::ast::MethodDefinitionKind::Constructor => {
                        product::MemberKind::Constructor
                    }
                    oxc_ast::ast::MethodDefinitionKind::Method => product::MemberKind::Method,
                    oxc_ast::ast::MethodDefinitionKind::Get => product::MemberKind::Getter,
                    oxc_ast::ast::MethodDefinitionKind::Set => product::MemberKind::Setter,
                };
                let name = Some(lower_property_name(context, &item.key, item.computed)?);
                let signature = lower_signature(
                    context,
                    item.value.type_parameters.as_deref(),
                    item.value.this_param.as_deref(),
                    &item.value.params,
                    item.value.return_type.as_deref(),
                    item.span,
                )?;
                let member = context.assembly.add_member(product::MemberRecipe {
                    site: context.oxc_site(item.span),
                    kind,
                    name,
                    optional: item.optional,
                    readonly: false,
                    static_member: item.r#static,
                    accessibility: lower_accessibility(item.accessibility),
                    r#abstract: matches!(
                        item.r#type,
                        oxc_ast::ast::MethodDefinitionType::TSAbstractMethodDefinition
                    ),
                    value: None,
                    signature: Some(signature),
                })?;
                members.push(member);
            }
            oxc_ast::ast::ClassElement::PropertyDefinition(item) => {
                let name = Some(lower_property_name(context, &item.key, item.computed)?);
                let value = item
                    .type_annotation
                    .as_deref()
                    .map(|annotation| lower_type(context, &annotation.type_annotation))
                    .transpose()?;
                let member = context.assembly.add_member(product::MemberRecipe {
                    site: context.oxc_site(item.span),
                    kind: product::MemberKind::Property,
                    name,
                    optional: item.optional,
                    readonly: item.readonly,
                    static_member: item.r#static,
                    accessibility: lower_accessibility(item.accessibility),
                    r#abstract: matches!(
                        item.r#type,
                        oxc_ast::ast::PropertyDefinitionType::TSAbstractPropertyDefinition
                    ),
                    value,
                    signature: None,
                })?;
                members.push(member);
            }
            oxc_ast::ast::ClassElement::TSIndexSignature(item) => {
                let signature = lower_index_signature(context, item)?;
                let member = context.assembly.add_member(product::MemberRecipe {
                    site: context.oxc_site(item.span),
                    kind: product::MemberKind::Index,
                    name: None,
                    optional: false,
                    readonly: item.readonly,
                    static_member: item.r#static,
                    accessibility: None,
                    r#abstract: false,
                    value: None,
                    signature: Some(signature),
                })?;
                members.push(member);
            }
            oxc_ast::ast::ClassElement::StaticBlock(item) => {
                return Err(context.unsupported(item.span, "class-static-block"))
            }
            oxc_ast::ast::ClassElement::AccessorProperty(item) => {
                return Err(context.unsupported(item.span, "class-accessor-property"))
            }
        }
    }
    let result = Ok(product::DeclarationPayload::Class {
        type_parameters,
        members: members.into_boxed_slice(),
        heritage: heritage.into_boxed_slice(),
    });
    context.active_type_parameters.pop();
    result
}

fn lower_declaration_type_parameters(
    context: &mut ProjectionContext<'_>,
    parameters: Option<&TSTypeParameterDeclaration<'_>>,
    owner_span: oxc_span::Span,
) -> Result<Box<[product::TypeParameterKey]>, LibraryProductProjectionError> {
    lower_type_parameters(context, parameters, owner_span)
}

fn lower_type(
    context: &mut ProjectionContext<'_>,
    ty: &TSType<'_>,
) -> Result<product::TypeRecipeKey, LibraryProductProjectionError> {
    let site = context.oxc_site(ty.span());
    let identity = (context.source_ordinal, site.start, site.end);
    if let Some(key) = context.type_by_site.get(&identity).copied() {
        return Ok(key);
    }
    let keyword = match ty {
        TSType::TSAnyKeyword(_) => Some(product::KeywordKind::Any),
        TSType::TSBigIntKeyword(_) => Some(product::KeywordKind::BigInt),
        TSType::TSBooleanKeyword(_) => Some(product::KeywordKind::Boolean),
        TSType::TSIntrinsicKeyword(_) => Some(product::KeywordKind::Intrinsic),
        TSType::TSNeverKeyword(_) => Some(product::KeywordKind::Never),
        TSType::TSNullKeyword(_) => Some(product::KeywordKind::Null),
        TSType::TSNumberKeyword(_) => Some(product::KeywordKind::Number),
        TSType::TSObjectKeyword(_) => Some(product::KeywordKind::Object),
        TSType::TSStringKeyword(_) => Some(product::KeywordKind::String),
        TSType::TSSymbolKeyword(_) => Some(product::KeywordKind::Symbol),
        TSType::TSThisType(_) => Some(product::KeywordKind::This),
        TSType::TSUndefinedKeyword(_) => Some(product::KeywordKind::Undefined),
        TSType::TSUnknownKeyword(_) => Some(product::KeywordKind::Unknown),
        TSType::TSVoidKeyword(_) => Some(product::KeywordKind::Void),
        _ => None,
    };
    let node = if let Some(keyword) = keyword {
        product::TypeRecipeNode::Keyword(keyword)
    } else {
        match ty {
            TSType::TSLiteralType(value) => product::TypeRecipeNode::Literal(lower_literal(
                context,
                &value.literal,
                value.span,
            )?),
            TSType::TSParenthesizedType(value) => {
                return lower_type(context, &value.type_annotation)
            }
            TSType::TSArrayType(value) => {
                product::TypeRecipeNode::Array(lower_type(context, &value.element_type)?)
            }
            TSType::TSUnionType(value) => {
                product::TypeRecipeNode::Union(lower_types(context, &value.types)?)
            }
            TSType::TSIntersectionType(value) => {
                product::TypeRecipeNode::Intersection(lower_types(context, &value.types)?)
            }
            TSType::TSConditionalType(value) => {
                let check = lower_type(context, &value.check_type)?;
                context
                    .active_type_parameters
                    .push(ActiveTypeParameterFrame {
                        kind: TypeParameterFrameKind::ConditionalInfer,
                        bindings: BTreeMap::new(),
                        owner: context.oxc_site(value.span),
                        visibility: vec![
                            context.oxc_site(value.extends_type.span()),
                            context.oxc_site(value.true_type.span()),
                        ]
                        .into_boxed_slice(),
                    });
                let extends = lower_type(context, &value.extends_type)?;
                let when_true = lower_type(context, &value.true_type)?;
                context.active_type_parameters.pop();
                product::TypeRecipeNode::Conditional {
                    check,
                    extends,
                    when_true,
                    when_false: lower_type(context, &value.false_type)?,
                }
            }
            TSType::TSIndexedAccessType(value) => product::TypeRecipeNode::IndexedAccess {
                object: lower_type(context, &value.object_type)?,
                index: lower_type(context, &value.index_type)?,
            },
            TSType::TSTypeOperatorType(value) => product::TypeRecipeNode::Operator {
                operator: match value.operator {
                    TSTypeOperatorOperator::Keyof => product::TypeOperator::Keyof,
                    TSTypeOperatorOperator::Readonly => product::TypeOperator::Readonly,
                    TSTypeOperatorOperator::Unique => product::TypeOperator::Unique,
                },
                operand: lower_type(context, &value.type_annotation)?,
            },
            TSType::TSTypeReference(value) => lower_reference_parts(
                context,
                &value.type_name,
                value.type_arguments.as_deref(),
                value.span,
            )?,
            TSType::TSTypeQuery(value) => {
                let (path_text, path) = match &value.expr_name {
                    oxc_ast::ast::TSTypeQueryExprName::IdentifierReference(name) => {
                        let key = context.assembly.intern(name.name.as_str())?;
                        (vec![name.name.as_str()], vec![key])
                    }
                    oxc_ast::ast::TSTypeQueryExprName::QualifiedName(name) => {
                        let mut path_text = type_name_segments(&name.left);
                        path_text.push(name.right.name.as_str());
                        let mut path = lower_qualified_path(context, &name.left)?;
                        path.push(context.assembly.intern(name.right.name.as_str())?);
                        (path_text, path)
                    }
                    oxc_ast::ast::TSTypeQueryExprName::ThisExpression(_)
                    | oxc_ast::ast::TSTypeQueryExprName::TSImportType(_) => {
                        return Err(context.unsupported(value.span, "type-query-expression"))
                    }
                };
                let resolution = resolve_value_reference(context, &path_text)?;
                let dependency = record_value_dependency(
                    context,
                    &path_text,
                    &path,
                    &resolution,
                    product::DependencyKind::Query,
                    value.span,
                )?;
                product::TypeRecipeNode::Query {
                    path: path.into_boxed_slice(),
                    resolution,
                    dependency,
                }
            }
            TSType::TSFunctionType(value) => product::TypeRecipeNode::Function(lower_signature(
                context,
                value.type_parameters.as_deref(),
                value.this_param.as_deref(),
                &value.params,
                Some(&value.return_type),
                value.span,
            )?),
            TSType::TSConstructorType(value) => {
                product::TypeRecipeNode::Constructor(lower_signature(
                    context,
                    value.type_parameters.as_deref(),
                    None,
                    &value.params,
                    Some(&value.return_type),
                    value.span,
                )?)
            }
            TSType::TSTypeLiteral(value) => {
                let mut members = Vec::with_capacity(value.members.len());
                for member in &value.members {
                    members.push(lower_member(context, member)?);
                }
                product::TypeRecipeNode::Object(members.into_boxed_slice())
            }
            TSType::TSTupleType(value) => product::TypeRecipeNode::Tuple {
                elements: lower_tuple(context, &value.element_types)?,
            },
            TSType::TSMappedType(value) => {
                let constraint = lower_type(context, &value.constraint)?;
                let parameter = lower_type_parameter_parts(
                    context,
                    value.key.name.as_str(),
                    value.key.span,
                    Some(constraint),
                    None,
                    TypeParameterModifiers::default(),
                    TypeParameterScope {
                        owner: context.oxc_site(value.span),
                        visibility: value
                            .name_type
                            .iter()
                            .map(|ty| context.oxc_site(ty.span()))
                            .chain(
                                value
                                    .type_annotation
                                    .iter()
                                    .map(|ty| context.oxc_site(ty.span())),
                            )
                            .collect::<Vec<_>>()
                            .into_boxed_slice(),
                    },
                )?;
                context
                    .active_type_parameters
                    .push(ActiveTypeParameterFrame {
                        kind: TypeParameterFrameKind::Lexical,
                        bindings: BTreeMap::from([(value.key.name.to_string(), parameter)]),
                        owner: context.oxc_site(value.span),
                        visibility: value
                            .name_type
                            .iter()
                            .map(|ty| context.oxc_site(ty.span()))
                            .chain(
                                value
                                    .type_annotation
                                    .iter()
                                    .map(|ty| context.oxc_site(ty.span())),
                            )
                            .collect::<Vec<_>>()
                            .into_boxed_slice(),
                    });
                let node = product::TypeRecipeNode::Mapped {
                    parameter,
                    name: value
                        .name_type
                        .as_ref()
                        .map(|ty| lower_type(context, ty))
                        .transpose()?,
                    value: value
                        .type_annotation
                        .as_ref()
                        .map(|ty| lower_type(context, ty))
                        .transpose()?,
                    optional: mapped_modifier(value.optional),
                    readonly: mapped_modifier(value.readonly),
                };
                context.active_type_parameters.pop();
                node
            }
            TSType::TSInferType(value) => {
                let frame_index = context
                    .active_type_parameters
                    .iter()
                    .rposition(|frame| frame.kind == TypeParameterFrameKind::ConditionalInfer)
                    .ok_or_else(|| context.unsupported(value.span, "infer-without-conditional"))?;
                let owner = context.active_type_parameters[frame_index].owner;
                let visibility = context.active_type_parameters[frame_index]
                    .visibility
                    .clone();
                let parameter =
                    lower_type_parameter(context, &value.type_parameter, owner, visibility)?;
                context.active_type_parameters[frame_index]
                    .bindings
                    .insert(value.type_parameter.name.name.to_string(), parameter);
                product::TypeRecipeNode::Infer(parameter)
            }
            TSType::TSTemplateLiteralType(value) => {
                let texts = value
                    .quasis
                    .iter()
                    .map(|item| context.assembly.intern(item.value.raw.as_str()))
                    .collect::<Result<Vec<_>, _>>()?;
                product::TypeRecipeNode::Template {
                    texts: texts.into_boxed_slice(),
                    holes: lower_types(context, &value.types)?,
                }
            }
            TSType::TSTypePredicate(value) => {
                let parameter = match &value.parameter_name {
                    oxc_ast::ast::TSTypePredicateName::Identifier(name) => {
                        context.assembly.intern(name.name.as_str())?
                    }
                    oxc_ast::ast::TSTypePredicateName::This(_) => {
                        context.assembly.intern("this")?
                    }
                };
                product::TypeRecipeNode::Predicate {
                    parameter,
                    asserts: value.asserts,
                    value: value
                        .type_annotation
                        .as_deref()
                        .map(|annotation| lower_type(context, &annotation.type_annotation))
                        .transpose()?,
                }
            }
            TSType::TSImportType(value) => {
                if value.options.is_some() {
                    return Err(context.unsupported(value.span, "import-type-options"));
                }
                let argument = context.assembly.intern(value.source.value.as_str())?;
                let qualifier = Vec::new();
                let arguments = value
                    .type_arguments
                    .as_deref()
                    .map(|items| lower_types(context, &items.params))
                    .transpose()?
                    .unwrap_or_default();
                product::TypeRecipeNode::Import {
                    argument,
                    qualifier: qualifier.into_boxed_slice(),
                    arguments,
                }
            }
            TSType::TSNamedTupleMember(value) => {
                let element = lower_tuple_element(
                    context,
                    &value.element_type,
                    Some(value.label.name.as_str()),
                    value.optional,
                )?;
                product::TypeRecipeNode::Tuple {
                    elements: vec![element].into_boxed_slice(),
                }
            }
            TSType::JSDocNullableType(value) => {
                return Err(context.unsupported(value.span, "jsdoc-nullable"))
            }
            TSType::JSDocNonNullableType(value) => {
                return Err(context.unsupported(value.span, "jsdoc-non-nullable"))
            }
            TSType::JSDocUnknownType(value) => {
                return Err(context.unsupported(value.span, "jsdoc-unknown"))
            }
            _ => unreachable!("keyword variants handled above"),
        }
    };
    let key = context
        .assembly
        .add_type_recipe(product::TypeRecipe { site, node })?;
    context.type_by_site.insert(identity, key);
    Ok(key)
}

fn lower_types(
    context: &mut ProjectionContext<'_>,
    types: &[TSType<'_>],
) -> Result<Box<[product::TypeRecipeKey]>, LibraryProductProjectionError> {
    types
        .iter()
        .map(|ty| lower_type(context, ty))
        .collect::<Result<Vec<_>, _>>()
        .map(Vec::into_boxed_slice)
}

fn lower_literal(
    context: &mut ProjectionContext<'_>,
    literal: &TSLiteral<'_>,
    span: oxc_span::Span,
) -> Result<product::LiteralRecipe, LibraryProductProjectionError> {
    match literal {
        TSLiteral::StringLiteral(value) => Ok(product::LiteralRecipe::String(
            context.assembly.intern(value.value.as_str())?,
        )),
        TSLiteral::NumericLiteral(value) => {
            Ok(product::LiteralRecipe::NumberBits(value.value.to_bits()))
        }
        TSLiteral::BooleanLiteral(value) => Ok(product::LiteralRecipe::Boolean(value.value)),
        TSLiteral::BigIntLiteral(value) => Ok(product::LiteralRecipe::BigInt(
            context.assembly.intern(value.value.as_str())?,
        )),
        TSLiteral::UnaryExpression(value) if value.operator.as_str() == "-" => {
            match &value.argument {
                oxc_ast::ast::Expression::NumericLiteral(number) => Ok(
                    product::LiteralRecipe::NumberBits((-number.value).to_bits()),
                ),
                _ => Err(context.unsupported(span, "literal-unary")),
            }
        }
        TSLiteral::TemplateLiteral(_) | TSLiteral::UnaryExpression(_) => {
            Err(context.unsupported(span, "literal"))
        }
    }
}

fn lower_reference_parts(
    context: &mut ProjectionContext<'_>,
    name: &TSTypeName<'_>,
    arguments: Option<&oxc_ast::ast::TSTypeParameterInstantiation<'_>>,
    span: oxc_span::Span,
) -> Result<product::TypeRecipeNode, LibraryProductProjectionError> {
    let path_text = type_name_segments(name);
    let path = lower_qualified_path(context, name)?;
    let resolution = resolve_type_reference(context, &path_text)?;
    let dependency = record_dependency(
        context,
        &path_text,
        &path,
        &resolution,
        product::DependencyKind::Reference,
        span,
    )?;
    Ok(product::TypeRecipeNode::Reference {
        path: path.into_boxed_slice(),
        resolution,
        arguments: arguments
            .map(|items| lower_types(context, &items.params))
            .transpose()?
            .unwrap_or_default(),
        dependency,
    })
}

fn lower_heritage(
    context: &mut ProjectionContext<'_>,
    expression: &oxc_ast::ast::Expression<'_>,
    arguments: Option<&oxc_ast::ast::TSTypeParameterInstantiation<'_>>,
    span: oxc_span::Span,
) -> Result<product::TypeRecipeNode, LibraryProductProjectionError> {
    let (path_text, path) = match expression {
        oxc_ast::ast::Expression::Identifier(value) => (
            vec![value.name.as_str()],
            vec![context.assembly.intern(value.name.as_str())?],
        ),
        oxc_ast::ast::Expression::StaticMemberExpression(value) => {
            let oxc_ast::ast::Expression::Identifier(object) = &value.object else {
                return Err(context.unsupported(span, "interface-heritage"));
            };
            (
                vec![object.name.as_str(), value.property.name.as_str()],
                vec![
                    context.assembly.intern(object.name.as_str())?,
                    context.assembly.intern(value.property.name.as_str())?,
                ],
            )
        }
        _ => return Err(context.unsupported(span, "interface-heritage")),
    };
    let resolution = resolve_type_reference(context, &path_text)?;
    let dependency = record_dependency(
        context,
        &path_text,
        &path,
        &resolution,
        product::DependencyKind::Heritage,
        span,
    )?;
    Ok(product::TypeRecipeNode::Reference {
        path: path.into_boxed_slice(),
        resolution,
        arguments: arguments
            .map(|items| lower_types(context, &items.params))
            .transpose()?
            .unwrap_or_default(),
        dependency,
    })
}

fn type_name_segments<'a>(name: &'a TSTypeName<'a>) -> Vec<&'a str> {
    match name {
        TSTypeName::IdentifierReference(value) => vec![value.name.as_str()],
        TSTypeName::QualifiedName(value) => {
            let mut path = type_name_segments(&value.left);
            path.push(value.right.name.as_str());
            path
        }
        TSTypeName::ThisExpression(_) => vec!["this"],
    }
}

fn record_dependency(
    context: &mut ProjectionContext<'_>,
    path: &[&str],
    path_keys: &[product::StringKey],
    resolution: &product::TypeReferenceResolution,
    kind: product::DependencyKind,
    span: oxc_span::Span,
) -> Result<product::DependencyLinkRecipe, LibraryProductProjectionError> {
    let owner =
        context
            .current_group
            .ok_or(LibraryProductProjectionError::MissingDependencyOwner {
                source: context.source_ordinal,
                start: span.start,
            })?;
    let scope =
        context
            .current_scope
            .ok_or(LibraryProductProjectionError::MissingAuthenticatedScope {
                source: context.source_ordinal,
                binding_start: span.start,
            })?;
    context.pending_dependencies.push(PendingDependency {
        owner,
        source_ordinal: context.source_ordinal,
        scope,
        region: context.current_region,
        path: path.iter().map(|segment| (*segment).to_owned()).collect(),
        path_keys: path_keys.to_vec(),
        target: match resolution {
            product::TypeReferenceResolution::Group(key) => PendingDependencyTarget::Group(*key),
            product::TypeReferenceResolution::TypeParameter(key) => {
                PendingDependencyTarget::TypeParameter(*key)
            }
            product::TypeReferenceResolution::Unresolved { .. } => {
                PendingDependencyTarget::Unresolved
            }
        },
        space: DependencySpace::Type,
        kind,
        site: context.oxc_site(span),
    });
    Ok(product::DependencyLinkRecipe { owner, kind })
}

fn record_value_dependency(
    context: &mut ProjectionContext<'_>,
    path: &[&str],
    path_keys: &[product::StringKey],
    resolution: &product::ValueReferenceResolution,
    kind: product::DependencyKind,
    span: oxc_span::Span,
) -> Result<product::DependencyLinkRecipe, LibraryProductProjectionError> {
    let owner =
        context
            .current_group
            .ok_or(LibraryProductProjectionError::MissingDependencyOwner {
                source: context.source_ordinal,
                start: span.start,
            })?;
    let scope =
        context
            .current_scope
            .ok_or(LibraryProductProjectionError::MissingAuthenticatedScope {
                source: context.source_ordinal,
                binding_start: span.start,
            })?;
    context.pending_dependencies.push(PendingDependency {
        owner,
        source_ordinal: context.source_ordinal,
        scope,
        region: context.current_region,
        path: path.iter().map(|segment| (*segment).to_owned()).collect(),
        path_keys: path_keys.to_vec(),
        target: match resolution {
            product::ValueReferenceResolution::Group(key) => PendingDependencyTarget::Group(*key),
            product::ValueReferenceResolution::IntrinsicGlobalValue => {
                PendingDependencyTarget::IntrinsicGlobalValue
            }
            product::ValueReferenceResolution::Unresolved { .. } => {
                PendingDependencyTarget::Unresolved
            }
        },
        space: DependencySpace::Value,
        kind,
        site: context.oxc_site(span),
    });
    Ok(product::DependencyLinkRecipe { owner, kind })
}

fn resolve_type_reference(
    context: &ProjectionContext<'_>,
    path: &[&str],
) -> Result<product::TypeReferenceResolution, LibraryProductProjectionError> {
    if let [name] = path {
        if let Some(parameter) = context
            .active_type_parameters
            .iter()
            .rev()
            .find_map(|scope| scope.bindings.get(*name).copied())
        {
            return Ok(product::TypeReferenceResolution::TypeParameter(parameter));
        }
    }
    let scope =
        context
            .current_scope
            .ok_or(LibraryProductProjectionError::MissingAuthenticatedScope {
                source: context.source_ordinal,
                binding_start: 0,
            })?;
    Ok(resolved_type_group(context.binder, scope, path)
        .and_then(|group| context.group_keys.get(&group).copied())
        .map(product::TypeReferenceResolution::Group)
        .unwrap_or(product::TypeReferenceResolution::Unresolved {
            region: context.current_region,
        }))
}

fn resolve_value_reference(
    context: &ProjectionContext<'_>,
    path: &[&str],
) -> Result<product::ValueReferenceResolution, LibraryProductProjectionError> {
    if path == ["globalThis"] {
        return Ok(product::ValueReferenceResolution::IntrinsicGlobalValue);
    }
    let scope =
        context
            .current_scope
            .ok_or(LibraryProductProjectionError::MissingAuthenticatedScope {
                source: context.source_ordinal,
                binding_start: 0,
            })?;
    Ok(resolved_value_storage(context.binder, scope, path)
        .and_then(|storage| context.value_group_keys.get(&storage.0).copied())
        .map(product::ValueReferenceResolution::Group)
        .unwrap_or(product::ValueReferenceResolution::Unresolved {
            region: context.current_region,
        }))
}

fn resolved_type_group(
    binder: &Binder,
    scope: ScopeId,
    path: &[impl AsRef<str>],
) -> Option<TypeGroupId> {
    let segments = path.iter().map(AsRef::as_ref).collect::<Vec<_>>();
    if segments.len() == 1 {
        return binder
            .resolve_type(scope, segments[0])
            .and_then(|symbol| binder.symbols.get(symbol))
            .and_then(|symbol| symbol.ty);
    }
    match binder.resolve_qualified_type_path(scope, &segments) {
        QualifiedTypePathResolution::TypeGroup(group) => Some(group),
        QualifiedTypePathResolution::MissingRoot { .. }
        | QualifiedTypePathResolution::TypeOnlyRoot { .. }
        | QualifiedTypePathResolution::MissingMember { .. }
        | QualifiedTypePathResolution::TypeOnlyIntermediate { .. }
        | QualifiedTypePathResolution::ValueOnlyLeaf { .. }
        | QualifiedTypePathResolution::Unavailable { .. }
        | QualifiedTypePathResolution::Deferred { .. } => None,
    }
}

fn resolved_value_storage(
    binder: &Binder,
    scope: ScopeId,
    path: &[impl AsRef<str>],
) -> Option<ValueStorageId> {
    let segments = path.iter().map(AsRef::as_ref).collect::<Vec<_>>();
    if segments.len() == 1 {
        return match binder.resolve_value_binding(scope, segments[0]) {
            super::bind::ValueResolution::Resolved { symbol, kind } => match kind {
                super::bind::ResolvedValueKind::Ordinary => {
                    binder.symbols.get(symbol).and_then(|symbol| symbol.value)
                }
                super::bind::ResolvedValueKind::StandaloneNamespace { storage, .. } => {
                    Some(storage)
                }
            },
            super::bind::ValueResolution::TypeOnlyNamespace { .. }
            | super::bind::ValueResolution::Missing => None,
        };
    }
    match binder.resolve_qualified_value_path(scope, &segments) {
        QualifiedValuePathResolution::Value(storage) => Some(storage),
        QualifiedValuePathResolution::MissingRoot { .. }
        | QualifiedValuePathResolution::TypeOnlyRoot { .. }
        | QualifiedValuePathResolution::MissingMember { .. }
        | QualifiedValuePathResolution::NamespaceOnlyLeaf { .. }
        | QualifiedValuePathResolution::Unavailable { .. }
        | QualifiedValuePathResolution::Deferred { .. } => None,
    }
}

fn lower_qualified_path(
    context: &mut ProjectionContext<'_>,
    name: &TSTypeName<'_>,
) -> Result<Vec<product::StringKey>, LibraryProductProjectionError> {
    match name {
        TSTypeName::IdentifierReference(value) => {
            Ok(vec![context.assembly.intern(value.name.as_str())?])
        }
        TSTypeName::QualifiedName(value) => {
            let mut path = lower_qualified_path(context, &value.left)?;
            path.push(context.assembly.intern(value.right.name.as_str())?);
            Ok(path)
        }
        TSTypeName::ThisExpression(_) => Ok(vec![context.assembly.intern("this")?]),
    }
}

fn lower_type_parameter(
    context: &mut ProjectionContext<'_>,
    parameter: &oxc_ast::ast::TSTypeParameter<'_>,
    owner: product::SourceSite,
    visibility: Box<[product::SourceSite]>,
) -> Result<product::TypeParameterKey, LibraryProductProjectionError> {
    let constraint = parameter
        .constraint
        .as_ref()
        .map(|ty| lower_type(context, ty))
        .transpose()?;
    let default = parameter
        .default
        .as_ref()
        .map(|ty| lower_type(context, ty))
        .transpose()?;
    lower_type_parameter_parts(
        context,
        parameter.name.name.as_str(),
        parameter.span,
        constraint,
        default,
        TypeParameterModifiers {
            input: parameter.r#in,
            output: parameter.out,
            constant: parameter.r#const,
        },
        TypeParameterScope { owner, visibility },
    )
}

#[derive(Clone, Copy, Default)]
struct TypeParameterModifiers {
    input: bool,
    output: bool,
    constant: bool,
}

struct TypeParameterScope {
    owner: product::SourceSite,
    visibility: Box<[product::SourceSite]>,
}

fn lower_type_parameter_parts(
    context: &mut ProjectionContext<'_>,
    name: &str,
    span: oxc_span::Span,
    constraint: Option<product::TypeRecipeKey>,
    default: Option<product::TypeRecipeKey>,
    modifiers: TypeParameterModifiers,
    scope: TypeParameterScope,
) -> Result<product::TypeParameterKey, LibraryProductProjectionError> {
    if let Some(key) = context
        .type_parameters
        .get(&(context.source_ordinal, span.start))
        .copied()
    {
        return Ok(key);
    }
    let name_text = name.to_owned();
    let site = context.oxc_site(span);
    let name = context.assembly.intern(name)?;
    let owner = scope.owner;
    let key = context
        .assembly
        .add_type_parameter(product::TypeParameterRecipe {
            site,
            owner: scope.owner,
            visibility: scope.visibility,
            name,
            variance: match (modifiers.input, modifiers.output) {
                (false, false) => product::Variance::Invariant,
                (true, false) => product::Variance::In,
                (false, true) => product::Variance::Out,
                (true, true) => product::Variance::InOut,
            },
            constant: modifiers.constant,
            constraint,
            default,
        })?;
    context.stable_type_parameter_anchors.insert(
        key,
        StableTypeParameterAnchor {
            site: StableSourceSite {
                source_ordinal: context.source_ordinal,
                start: site.start,
                end: site.end,
            },
            owner: StableSourceSite {
                source_ordinal: context.source_ordinal,
                start: owner.start,
                end: owner.end,
            },
            name: name_text,
        },
    );
    context
        .type_parameters
        .insert((context.source_ordinal, span.start), key);
    Ok(key)
}

fn lower_type_parameters(
    context: &mut ProjectionContext<'_>,
    parameters: Option<&TSTypeParameterDeclaration<'_>>,
    owner_span: oxc_span::Span,
) -> Result<Box<[product::TypeParameterKey]>, LibraryProductProjectionError> {
    let owner = context.oxc_site(owner_span);
    context
        .active_type_parameters
        .push(ActiveTypeParameterFrame {
            kind: TypeParameterFrameKind::Lexical,
            bindings: BTreeMap::new(),
            owner,
            visibility: vec![owner].into_boxed_slice(),
        });
    let mut keys = Vec::new();
    for parameter in parameters
        .into_iter()
        .flat_map(|parameters| parameters.params.iter())
    {
        let key = lower_type_parameter(context, parameter, owner, vec![owner].into_boxed_slice())?;
        context
            .active_type_parameters
            .last_mut()
            .ok_or(LibraryProductProjectionError::MissingPayload {
                source: context.source_ordinal,
                binding_start: parameter.span.start,
            })?
            .bindings
            .insert(parameter.name.name.to_string(), key);
        keys.push(key);
    }
    Ok(keys.into_boxed_slice())
}

fn lower_signature(
    context: &mut ProjectionContext<'_>,
    type_parameters: Option<&TSTypeParameterDeclaration<'_>>,
    this_parameter: Option<&oxc_ast::ast::TSThisParameter<'_>>,
    parameters: &FormalParameters<'_>,
    result: Option<&oxc_ast::ast::TSTypeAnnotation<'_>>,
    owner_span: oxc_span::Span,
) -> Result<product::SignatureKey, LibraryProductProjectionError> {
    let type_parameters = lower_type_parameters(context, type_parameters, owner_span)?;
    let mut lowered = Vec::new();
    if let Some(this_parameter) = this_parameter {
        lowered.push(product::ParameterRecipe {
            pattern: product::ParameterPattern::Identifier(context.assembly.intern("this")?),
            value: this_parameter
                .type_annotation
                .as_deref()
                .map(|value| lower_type(context, &value.type_annotation))
                .transpose()?,
            optional: false,
            rest: false,
            this_parameter: true,
        });
    }
    for parameter in &parameters.items {
        let Some(name) = parameter.pattern.get_identifier_name() else {
            return Err(context.unsupported(parameter.span, "parameter-pattern"));
        };
        lowered.push(product::ParameterRecipe {
            pattern: product::ParameterPattern::Identifier(context.assembly.intern(name.as_str())?),
            value: parameter
                .type_annotation
                .as_deref()
                .map(|value| lower_type(context, &value.type_annotation))
                .transpose()?,
            optional: parameter.optional,
            rest: false,
            this_parameter: false,
        });
    }
    if let Some(rest) = &parameters.rest {
        let pattern = lower_parameter_pattern(context, &rest.rest.argument)?;
        lowered.push(product::ParameterRecipe {
            pattern,
            value: rest
                .type_annotation
                .as_deref()
                .map(|value| lower_type(context, &value.type_annotation))
                .transpose()?,
            optional: false,
            rest: true,
            this_parameter: false,
        });
    }
    let result = result
        .map(|value| lower_type(context, &value.type_annotation))
        .transpose()?;
    let signature = context
        .assembly
        .add_signature(product::SignatureRecipe {
            type_parameters,
            parameters: lowered.into_boxed_slice(),
            result,
        })
        .map_err(Into::into);
    context.active_type_parameters.pop();
    signature
}

fn lower_parameter_pattern(
    context: &mut ProjectionContext<'_>,
    pattern: &oxc_ast::ast::BindingPattern<'_>,
) -> Result<product::ParameterPattern, LibraryProductProjectionError> {
    match pattern {
        oxc_ast::ast::BindingPattern::BindingIdentifier(value) => Ok(
            product::ParameterPattern::Identifier(context.assembly.intern(value.name.as_str())?),
        ),
        oxc_ast::ast::BindingPattern::ArrayPattern(_) => Ok(product::ParameterPattern::Array(
            lower_pattern_names(context, pattern)?.into_boxed_slice(),
        )),
        oxc_ast::ast::BindingPattern::ObjectPattern(_) => Ok(product::ParameterPattern::Object(
            lower_pattern_names(context, pattern)?.into_boxed_slice(),
        )),
        oxc_ast::ast::BindingPattern::AssignmentPattern(_) => {
            Err(context.unsupported(pattern.span(), "assignment-parameter-pattern"))
        }
    }
}

fn lower_pattern_names(
    context: &mut ProjectionContext<'_>,
    pattern: &oxc_ast::ast::BindingPattern<'_>,
) -> Result<Vec<product::StringKey>, LibraryProductProjectionError> {
    let mut names = Vec::new();
    for identifier in pattern.get_binding_identifiers() {
        names.push(context.assembly.intern(identifier.name.as_str())?);
    }
    Ok(names)
}

fn lower_member(
    context: &mut ProjectionContext<'_>,
    member: &TSSignature<'_>,
) -> Result<product::MemberKey, LibraryProductProjectionError> {
    let (site, kind, name, optional, readonly, value, signature) = match member {
        TSSignature::TSPropertySignature(item) => (
            item.span,
            product::MemberKind::Property,
            Some(lower_property_name(context, &item.key, item.computed)?),
            item.optional,
            item.readonly,
            item.type_annotation
                .as_deref()
                .map(|value| lower_type(context, &value.type_annotation))
                .transpose()?,
            None,
        ),
        TSSignature::TSMethodSignature(item) => (
            item.span,
            match item.kind {
                oxc_ast::ast::TSMethodSignatureKind::Method => product::MemberKind::Method,
                oxc_ast::ast::TSMethodSignatureKind::Get => product::MemberKind::Getter,
                oxc_ast::ast::TSMethodSignatureKind::Set => product::MemberKind::Setter,
            },
            Some(lower_property_name(context, &item.key, item.computed)?),
            item.optional,
            false,
            None,
            Some(lower_signature(
                context,
                item.type_parameters.as_deref(),
                item.this_param.as_deref(),
                &item.params,
                item.return_type.as_deref(),
                item.span,
            )?),
        ),
        TSSignature::TSCallSignatureDeclaration(item) => (
            item.span,
            product::MemberKind::Call,
            None,
            false,
            false,
            None,
            Some(lower_signature(
                context,
                item.type_parameters.as_deref(),
                item.this_param.as_deref(),
                &item.params,
                item.return_type.as_deref(),
                item.span,
            )?),
        ),
        TSSignature::TSConstructSignatureDeclaration(item) => (
            item.span,
            product::MemberKind::Construct,
            None,
            false,
            false,
            None,
            Some(lower_signature(
                context,
                item.type_parameters.as_deref(),
                None,
                &item.params,
                item.return_type.as_deref(),
                item.span,
            )?),
        ),
        TSSignature::TSIndexSignature(item) => {
            let signature = lower_index_signature(context, item)?;
            (
                item.span,
                product::MemberKind::Index,
                None,
                false,
                item.readonly,
                None,
                Some(signature),
            )
        }
    };
    context
        .assembly
        .add_member(product::MemberRecipe {
            site: context.oxc_site(site),
            kind,
            name,
            optional,
            readonly,
            static_member: false,
            accessibility: None,
            r#abstract: false,
            value,
            signature,
        })
        .map_err(Into::into)
}

fn lower_index_signature(
    context: &mut ProjectionContext<'_>,
    item: &oxc_ast::ast::TSIndexSignature<'_>,
) -> Result<product::SignatureKey, LibraryProductProjectionError> {
    let parameters = item
        .parameters
        .iter()
        .map(|parameter| {
            Ok(product::ParameterRecipe {
                pattern: product::ParameterPattern::Identifier(
                    context.assembly.intern(parameter.name.as_str())?,
                ),
                value: Some(lower_type(
                    context,
                    &parameter.type_annotation.type_annotation,
                )?),
                optional: false,
                rest: false,
                this_parameter: false,
            })
        })
        .collect::<Result<Vec<_>, LibraryProductProjectionError>>()?;
    let result = lower_type(context, &item.type_annotation.type_annotation)?;
    context
        .assembly
        .add_signature(product::SignatureRecipe {
            type_parameters: Box::new([]),
            parameters: parameters.into_boxed_slice(),
            result: Some(result),
        })
        .map_err(Into::into)
}

fn lower_accessibility(
    value: Option<oxc_ast::ast::TSAccessibility>,
) -> Option<product::Accessibility> {
    value.map(|value| match value {
        oxc_ast::ast::TSAccessibility::Public => product::Accessibility::Public,
        oxc_ast::ast::TSAccessibility::Protected => product::Accessibility::Protected,
        oxc_ast::ast::TSAccessibility::Private => product::Accessibility::Private,
    })
}

fn lower_property_name(
    context: &mut ProjectionContext<'_>,
    key: &PropertyKey<'_>,
    computed: bool,
) -> Result<product::PropertyNameRecipe, LibraryProductProjectionError> {
    if !computed {
        return match key {
            PropertyKey::StaticIdentifier(value) => Ok(product::PropertyNameRecipe::Identifier(
                context.assembly.intern(value.name.as_str())?,
            )),
            PropertyKey::PrivateIdentifier(value) => Ok(product::PropertyNameRecipe::Private(
                context.assembly.intern(value.name.as_str())?,
            )),
            PropertyKey::StringLiteral(value) => Ok(product::PropertyNameRecipe::String(
                context.assembly.intern(value.value.as_str())?,
            )),
            PropertyKey::NumericLiteral(value) => Ok(product::PropertyNameRecipe::NumberBits(
                value.value.to_bits(),
            )),
            _ => Err(LibraryProductProjectionError::UnsupportedProperty {
                source: context.source_ordinal,
                start: key.span().start,
            }),
        };
    }
    let symbolic = match key {
        PropertyKey::Identifier(value) => {
            let path_text = vec![value.name.as_str()];
            let path = vec![context.assembly.intern(value.name.as_str())?];
            let resolution = resolve_value_reference(context, &path_text)?;
            let dependency = record_value_dependency(
                context,
                &path_text,
                &path,
                &resolution,
                product::DependencyKind::Query,
                key.span(),
            )?;
            product::SymbolicValueNameRecipe::Path {
                path: path.into_boxed_slice(),
                resolution,
                dependency,
            }
        }
        PropertyKey::StaticMemberExpression(value) => {
            let mut path_text = value_name_text_path(context.source_ordinal, &value.object)?;
            path_text.push(value.property.name.as_str());
            let mut path = lower_value_name_path(context, &value.object)?;
            path.push(context.assembly.intern(value.property.name.as_str())?);
            let resolution = resolve_value_reference(context, &path_text)?;
            let dependency = record_value_dependency(
                context,
                &path_text,
                &path,
                &resolution,
                product::DependencyKind::Query,
                key.span(),
            )?;
            product::SymbolicValueNameRecipe::Path {
                path: path.into_boxed_slice(),
                resolution,
                dependency,
            }
        }
        PropertyKey::StringLiteral(value) => {
            product::SymbolicValueNameRecipe::String(context.assembly.intern(value.value.as_str())?)
        }
        PropertyKey::NumericLiteral(value) => {
            product::SymbolicValueNameRecipe::NumberBits(value.value.to_bits())
        }
        _ => {
            return Err(LibraryProductProjectionError::UnsupportedProperty {
                source: context.source_ordinal,
                start: key.span().start,
            })
        }
    };
    Ok(product::PropertyNameRecipe::Computed {
        site: context.oxc_site(key.span()),
        value: symbolic,
    })
}

fn value_name_text_path<'a>(
    source: u32,
    expression: &'a oxc_ast::ast::Expression<'a>,
) -> Result<Vec<&'a str>, LibraryProductProjectionError> {
    match expression {
        oxc_ast::ast::Expression::Identifier(value) => Ok(vec![value.name.as_str()]),
        oxc_ast::ast::Expression::StaticMemberExpression(value) => {
            let mut path = value_name_text_path(source, &value.object)?;
            path.push(value.property.name.as_str());
            Ok(path)
        }
        _ => Err(LibraryProductProjectionError::UnsupportedProperty {
            source,
            start: expression.span().start,
        }),
    }
}

fn lower_value_name_path(
    context: &mut ProjectionContext<'_>,
    expression: &oxc_ast::ast::Expression<'_>,
) -> Result<Vec<product::StringKey>, LibraryProductProjectionError> {
    match expression {
        oxc_ast::ast::Expression::Identifier(value) => {
            Ok(vec![context.assembly.intern(value.name.as_str())?])
        }
        oxc_ast::ast::Expression::StaticMemberExpression(value) => {
            let mut path = lower_value_name_path(context, &value.object)?;
            path.push(context.assembly.intern(value.property.name.as_str())?);
            Ok(path)
        }
        _ => Err(LibraryProductProjectionError::UnsupportedProperty {
            source: context.source_ordinal,
            start: expression.span().start,
        }),
    }
}

fn lower_tuple(
    context: &mut ProjectionContext<'_>,
    elements: &[TSTupleElement<'_>],
) -> Result<Box<[product::TupleElementRecipe]>, LibraryProductProjectionError> {
    elements
        .iter()
        .map(|element| lower_tuple_element(context, element, None, false))
        .collect::<Result<Vec<_>, _>>()
        .map(Vec::into_boxed_slice)
}

fn lower_tuple_element(
    context: &mut ProjectionContext<'_>,
    element: &TSTupleElement<'_>,
    label: Option<&str>,
    inherited_optional: bool,
) -> Result<product::TupleElementRecipe, LibraryProductProjectionError> {
    let (value, optional, rest) = match element {
        TSTupleElement::TSOptionalType(value) => {
            (lower_type(context, &value.type_annotation)?, true, false)
        }
        TSTupleElement::TSRestType(value) => {
            (lower_type(context, &value.type_annotation)?, false, true)
        }
        TSTupleElement::TSNamedTupleMember(value) => {
            return lower_tuple_element(
                context,
                &value.element_type,
                Some(value.label.name.as_str()),
                value.optional,
            )
        }
        _ => (
            lower_type(context, element.to_ts_type())?,
            inherited_optional,
            false,
        ),
    };
    Ok(product::TupleElementRecipe {
        name: label
            .map(|name| context.assembly.intern(name))
            .transpose()?,
        value,
        optional: optional || inherited_optional,
        rest,
    })
}

fn mapped_modifier(value: Option<TSMappedTypeModifierOperator>) -> product::MappedModifier {
    match value {
        None | Some(TSMappedTypeModifierOperator::True) => product::MappedModifier::Preserve,
        Some(TSMappedTypeModifierOperator::Plus) => product::MappedModifier::Add,
        Some(TSMappedTypeModifierOperator::Minus) => product::MappedModifier::Remove,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binder::bind::ProjectBinderBuilder;
    use crate::binder::namespace::{
        source_file_kind, CompilationUnit, ModuleBindingContext, SourceUnitKey,
    };
    use crate::source::{CompilationOrigin, LibraryFileOrdinal};
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    #[test]
    fn projection_resolves_nested_duplicate_names_and_type_parameters_exactly() {
        let source = r#"
            declare namespace Outer {
                interface X {}
                interface Y extends X { value: X }
                namespace Inner {
                    interface X {}
                    interface Y extends X { value: X }
                }
            }
            interface Box<T> { value: T }
            type Wrapped<U> = Box<U>;
        "#;
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::d_ts()).parse();
        assert!(!parsed.panicked);
        assert!(parsed.diagnostics.is_empty());
        let manifest = SourceDeclarationManifest::build(&parsed.program);
        let prelude_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
        let mut builder = ProjectBinderBuilder::new_empty_library_prelude(&prelude.program);
        let unit = CompilationUnit {
            source: SourceUnitKey(1),
            origin: CompilationOrigin::Library(LibraryFileOrdinal::new(0)),
            binding: ModuleBindingContext::for_program(
                &parsed.program,
                source_file_kind("nested.d.ts"),
            ),
        };
        let modules = builder
            .try_add_library_modules_with_declaration_manifests(&[(
                &parsed.program,
                &manifest,
                unit,
            )])
            .expect("bind exact source");
        let binder = builder.finish(modules[0]);
        let product = project_library_declaration_product(
            "nested-control",
            &binder,
            &[LibraryProductSource {
                ordinal: 0,
                name: "nested.d.ts",
                source_sha256: "00",
                byte_len: u32::try_from(source.len()).expect("source length"),
                external_module: false,
                module: modules[0],
                manifest: &manifest,
                source_text: source,
            }],
        )
        .expect("project exact source");
        let heritage_targets = product
            .dependency_rows()
            .iter()
            .filter(|row| row.kind == product::DependencyKind::Heritage)
            .filter_map(|row| match &row.target {
                product::DependencyTarget::Group(group) => Some(*group),
                product::DependencyTarget::TypeParameter(_)
                | product::DependencyTarget::IntrinsicGlobalValue
                | product::DependencyTarget::Unresolved { .. } => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(heritage_targets.len(), 2);
        assert_ne!(heritage_targets[0], heritage_targets[1]);
        let receipt = product.audit_receipt();
        assert!(receipt.type_parameter_reference_count >= 2);
        assert!(receipt.type_parameter_dependency_count >= 2);
    }

    #[test]
    fn conditional_infer_uses_nearest_conditional_frame_across_nested_type_forms() {
        let source = r#"
            type Cases<T> = T extends infer P
                ? {
                    function: (value: P) => P;
                    constructor: new (value: P) => P;
                    object: { value: P };
                    shadow: <P>(value: P) => P;
                    mapped: { [K in keyof P]: P };
                    nested: P extends infer R ? (() => P | R) : never;
                }
                : P;
        "#;
        let product = project_test_source("infer.d.ts", source);
        let parameter_name = |key: product::TypeParameterKey| {
            let parameter = &product.type_parameters()[key.index()];
            product.strings()[parameter.name.index()].as_str()
        };
        let infer_p = product
            .types()
            .iter()
            .find_map(|recipe| match recipe.node {
                product::TypeRecipeNode::Infer(key) if parameter_name(key) == "P" => Some(key),
                product::TypeRecipeNode::Keyword(_)
                | product::TypeRecipeNode::Literal(_)
                | product::TypeRecipeNode::Reference { .. }
                | product::TypeRecipeNode::Query { .. }
                | product::TypeRecipeNode::Import { .. }
                | product::TypeRecipeNode::Array(_)
                | product::TypeRecipeNode::Tuple { .. }
                | product::TypeRecipeNode::Union(_)
                | product::TypeRecipeNode::Intersection(_)
                | product::TypeRecipeNode::Function(_)
                | product::TypeRecipeNode::Constructor(_)
                | product::TypeRecipeNode::Object(_)
                | product::TypeRecipeNode::Operator { .. }
                | product::TypeRecipeNode::IndexedAccess { .. }
                | product::TypeRecipeNode::Mapped { .. }
                | product::TypeRecipeNode::Conditional { .. }
                | product::TypeRecipeNode::Infer(_)
                | product::TypeRecipeNode::Template { .. }
                | product::TypeRecipeNode::Predicate { .. } => None,
            })
            .expect("outer infer P");
        let mut infer_p_references = 0;
        let mut shadow_p_references = Vec::new();
        let mut unresolved_p_references = 0;
        for recipe in product.types() {
            let product::TypeRecipeNode::Reference {
                path,
                resolution,
                arguments: _,
                dependency: _,
            } = &recipe.node
            else {
                continue;
            };
            if path.len() != 1 || product.strings()[path[0].index()] != "P" {
                continue;
            }
            match resolution {
                product::TypeReferenceResolution::TypeParameter(key) if *key == infer_p => {
                    infer_p_references += 1;
                }
                product::TypeReferenceResolution::TypeParameter(key) => {
                    shadow_p_references.push(*key);
                }
                product::TypeReferenceResolution::Unresolved { region: _ } => {
                    unresolved_p_references += 1;
                }
                product::TypeReferenceResolution::Group(_) => {
                    panic!("P must not bind to an ordinary group")
                }
            }
        }
        assert_eq!(infer_p_references, 9);
        assert_eq!(shadow_p_references.len(), 2);
        assert!(shadow_p_references
            .iter()
            .all(|key| *key == shadow_p_references[0]));
        assert_ne!(shadow_p_references[0], infer_p);
        assert_eq!(unresolved_p_references, 1);
        let infer_r = product
            .types()
            .iter()
            .find_map(|recipe| match recipe.node {
                product::TypeRecipeNode::Infer(key) if parameter_name(key) == "R" => Some(key),
                _ => None,
            })
            .expect("nested infer R");
        assert!(product.types().iter().any(|recipe| {
            matches!(
                &recipe.node,
                product::TypeRecipeNode::Reference {
                    resolution: product::TypeReferenceResolution::TypeParameter(key),
                    ..
                } if *key == infer_r
            )
        }));
    }

    #[test]
    fn value_queries_use_value_space_for_simple_qualified_and_computed_paths() {
        let source = r#"
            declare var globalThis: { marker: number };
            declare var Symbol: { readonly iterator: unique symbol };
            declare var Atomics: { wait: () => void };
            declare namespace Bag { const item: unique symbol; }
            interface Uses {
                simple: typeof globalThis;
                structural: typeof Atomics.wait;
                [Symbol.iterator](): void;
                [Bag.item]: number;
            }
        "#;
        let product = project_test_source("value-query.d.ts", source);
        let path_text = |path: &[product::StringKey]| {
            path.iter()
                .map(|key| product.strings()[key.index()].as_str())
                .collect::<Vec<_>>()
        };
        let query_paths = product
            .types()
            .iter()
            .filter_map(|recipe| match &recipe.node {
                product::TypeRecipeNode::Query {
                    path,
                    resolution,
                    dependency: _,
                } => Some((path_text(path), resolution)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let expected = ["Atomics", "wait"].as_slice();
        let (_, resolution) = query_paths
            .iter()
            .find(|(path, _)| path == expected)
            .expect("query path");
        assert!(
            matches!(resolution, product::ValueReferenceResolution::Group(_)),
            "{expected:?}: {resolution:?}"
        );
        let (_, global_this_resolution) = query_paths
            .iter()
            .find(|(path, _)| path == &["globalThis"])
            .expect("globalThis query path");
        assert_eq!(
            **global_this_resolution,
            product::ValueReferenceResolution::IntrinsicGlobalValue
        );
        assert_eq!(
            product
                .dependency_rows()
                .iter()
                .filter(|dependency| {
                    dependency.kind == product::DependencyKind::Query
                        && dependency.target == product::DependencyTarget::IntrinsicGlobalValue
                })
                .count(),
            1
        );
        let computed_paths = product
            .members()
            .iter()
            .filter_map(|member| match &member.name {
                Some(product::PropertyNameRecipe::Computed {
                    site: _,
                    value:
                        product::SymbolicValueNameRecipe::Path {
                            path,
                            resolution,
                            dependency: _,
                        },
                }) => Some((path_text(path), resolution)),
                Some(
                    product::PropertyNameRecipe::Identifier(_)
                    | product::PropertyNameRecipe::String(_)
                    | product::PropertyNameRecipe::NumberBits(_)
                    | product::PropertyNameRecipe::Private(_)
                    | product::PropertyNameRecipe::Computed {
                        site: _,
                        value:
                            product::SymbolicValueNameRecipe::String(_)
                            | product::SymbolicValueNameRecipe::NumberBits(_),
                    },
                )
                | None => None,
            })
            .collect::<Vec<_>>();
        for expected in [
            ["Symbol", "iterator"].as_slice(),
            ["Bag", "item"].as_slice(),
        ] {
            let (_, resolution) = computed_paths
                .iter()
                .find(|(path, _)| path == expected)
                .expect("computed path");
            assert!(
                matches!(resolution, product::ValueReferenceResolution::Group(_)),
                "{expected:?}: {resolution:?}"
            );
        }
    }

    #[test]
    #[ignore = "RED until the WU2 intrinsic/cooked product contract is implemented"]
    fn template_recipe_authenticates_cooked_segments_not_raw_escape_spelling() {
        let escaped_source = r#"type Escaped<T> = `start\n${T}\u0041`;"#;
        let braced_source = r#"type Escaped<T> = `start\n${T}\u{41}`;"#;
        let different_source = r#"type Escaped<T> = `start\n${T}\u{42}`;"#;
        assert_eq!(escaped_source.len(), braced_source.len());
        assert_eq!(braced_source.len(), different_source.len());
        let escaped = project_test_source("template-cooked.d.ts", escaped_source);
        let braced = project_test_source("template-cooked.d.ts", braced_source);
        let different = project_test_source("template-cooked.d.ts", different_source);
        escaped.validate().expect("escaped product validates");
        braced.validate().expect("braced product validates");
        different.validate().expect("different product validates");

        let template_text = |product: &product::LibraryDeclarationProduct| {
            let texts = product
                .types()
                .iter()
                .find_map(|recipe| match &recipe.node {
                    product::TypeRecipeNode::Template { texts, holes: _ } => Some(texts),
                    _ => None,
                })
                .expect("template type recipe");
            texts
                .iter()
                .map(|key| product.strings()[key.index()].clone())
                .collect::<Vec<_>>()
        };
        let cooked = ["start\n".to_owned(), "A".to_owned()];
        assert_ne!(cooked, [r"start\n".to_owned(), r"\u0041".to_owned()]);
        assert_eq!(template_text(&escaped), cooked);
        assert_eq!(template_text(&braced), cooked);
        assert_eq!(escaped.types(), braced.types());

        let canonical = escaped.canonical_audit_render();
        assert_eq!(canonical, braced.canonical_audit_render());
        assert_eq!(
            escaped.audit_receipt().canonical_digest,
            product::sha256_hex(canonical.as_bytes())
        );
        assert_eq!(
            escaped.audit_receipt().canonical_digest,
            braced.audit_receipt().canonical_digest
        );
        assert_ne!(
            escaped.audit_receipt().canonical_digest,
            different.audit_receipt().canonical_digest
        );
    }

    #[test]
    fn ordinary_alias_names_cannot_certify_intrinsic_markers() {
        const PACKAGED_PROFILE: &str =
            "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";
        for (control, name, ordinal, byte_len, trusted_sha256, source_body) in [
            (
                "ordinary aliases",
                "lib.es5.d.ts",
                0usize,
                218_972usize,
                "bcd24271a113971ba9eb71ff8cb01bc6b0f872a85c23fdbe5d93065b375933cd",
                r#"
                    type Uppercase<S extends string> = string;
                    type Lowercase<S extends string> = string;
                    type Capitalize<S extends string> = string;
                    type Uncapitalize<S extends string> = string;
                    type NoInfer<T> = T;
                "#,
            ),
            (
                "intrinsic aliases",
                "lib.es5.d.ts",
                0usize,
                218_972usize,
                "bcd24271a113971ba9eb71ff8cb01bc6b0f872a85c23fdbe5d93065b375933cd",
                r#"
                    type Uppercase<S extends string> = intrinsic;
                    type Lowercase<S extends string> = intrinsic;
                    type Capitalize<S extends string> = intrinsic;
                    type Uncapitalize<S extends string> = intrinsic;
                    type NoInfer<T> = intrinsic;
                    type UntrustedIntrinsic<T> = intrinsic;
                "#,
            ),
            (
                "ordinary alias",
                "lib.es2015.iterable.d.ts",
                20usize,
                18_185usize,
                "ebd9f816d4002697cb2864bea1f0b70a103124e18a8cd9645eeccc09bdf80ab4",
                "type BuiltinIteratorReturn = string;",
            ),
            (
                "intrinsic alias",
                "lib.es2015.iterable.d.ts",
                20usize,
                18_185usize,
                "ebd9f816d4002697cb2864bea1f0b70a103124e18a8cd9645eeccc09bdf80ab4",
                "type BuiltinIteratorReturn = intrinsic; type UntrustedIntrinsic = intrinsic;",
            ),
        ] {
            let source = padded_source(source_body, byte_len);
            for claimed_sha256 in ["wrong-source-sha256", trusted_sha256] {
                let product = project_test_source_with_identity(
                    name,
                    &source,
                    PACKAGED_PROFILE,
                    claimed_sha256,
                    ordinal,
                );
                product.validate().expect("control product validates");
                let source_row = &product.sources()[0];
                assert_eq!(source_row.ordinal, u32::try_from(ordinal).expect("ordinal"));
                assert_eq!(
                    source_row.byte_len,
                    u32::try_from(byte_len).expect("length")
                );
                assert_eq!(product.strings()[source_row.sha256.index()], claimed_sha256);
                assert!(
                    product.intrinsic_marker_rows().is_empty(),
                    "{control}:{name}:{claimed_sha256}"
                );
            }
        }
    }

    fn padded_source(body: &str, byte_len: usize) -> String {
        let padding = byte_len
            .checked_sub(body.len().saturating_add(5))
            .expect("fake source fits official byte length");
        let source = format!("{body}\n/*{}*/", " ".repeat(padding));
        assert_eq!(source.len(), byte_len);
        source
    }

    fn project_test_source(name: &str, source: &str) -> product::LibraryDeclarationProduct {
        project_test_source_with_identity(name, source, "test-control", "00", 0)
    }

    fn project_test_source_with_identity(
        name: &str,
        source: &str,
        profile_identity: &str,
        source_sha256: &str,
        ordinal: usize,
    ) -> product::LibraryDeclarationProduct {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::d_ts()).parse();
        assert!(!parsed.panicked);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let manifest = SourceDeclarationManifest::build(&parsed.program);
        let prelude_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
        let mut builder = ProjectBinderBuilder::new_empty_library_prelude(&prelude.program);
        let ordinal_u32 = u32::try_from(ordinal).expect("source ordinal");
        let unit = CompilationUnit {
            source: SourceUnitKey(ordinal_u32.checked_add(1).expect("source key")),
            origin: CompilationOrigin::Library(LibraryFileOrdinal::new(ordinal)),
            binding: ModuleBindingContext::for_program(&parsed.program, source_file_kind(name)),
        };
        let modules = builder
            .try_add_library_modules_with_declaration_manifests(&[(
                &parsed.program,
                &manifest,
                unit,
            )])
            .expect("bind exact source");
        let binder = builder.finish(modules[0]);
        project_library_declaration_product(
            profile_identity,
            &binder,
            &[LibraryProductSource {
                ordinal: ordinal_u32,
                name,
                source_sha256,
                byte_len: u32::try_from(source.len()).expect("source length"),
                external_module: false,
                module: modules[0],
                manifest: &manifest,
                source_text: source,
            }],
        )
        .expect("project exact source")
    }
}

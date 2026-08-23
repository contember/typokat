use crate::schema::{
    DeclarationPayload, DependencyKind, DependencyTarget, LibraryDeclarationProduct, LiteralRecipe,
    ParameterPattern, PropertyNameRecipe, SymbolicValueNameRecipe, TypeRecipeNode,
    TypeReferenceResolution, ValueReferenceResolution,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProductValidationError {
    EmptyProfile,
    EmptySources,
    InvalidSourceSite {
        table: &'static str,
        row: usize,
    },
    InvalidString {
        table: &'static str,
        row: usize,
    },
    InvalidRegion {
        table: &'static str,
        row: usize,
    },
    InvalidContainer {
        table: &'static str,
        row: usize,
    },
    InvalidDeclaration {
        table: &'static str,
        row: usize,
    },
    InvalidGroup {
        table: &'static str,
        row: usize,
    },
    InvalidTypeRecipe {
        table: &'static str,
        row: usize,
    },
    InvalidSignature {
        row: usize,
    },
    InvalidMember {
        row: usize,
    },
    InvalidAttachment {
        table: &'static str,
        row: usize,
    },
    InvalidRegionContainment {
        row: usize,
    },
    DuplicateAttachment {
        table: &'static str,
        row: usize,
    },
    NonCanonicalSourceOrder {
        row: usize,
    },
    NonCanonicalFragmentOrder {
        row: usize,
    },
    GroupFragmentName {
        group: usize,
        declaration: usize,
    },
    GroupFragmentContainer {
        group: usize,
        declaration: usize,
    },
    MissingAttachment {
        table: &'static str,
        group: usize,
        declaration: usize,
    },
    ExtraAttachment {
        table: &'static str,
        row: usize,
    },
    InvalidChildRegion {
        declaration: usize,
    },
    InvalidLexicalStep {
        row: usize,
    },
    DependencyReferenceMismatch {
        row: usize,
    },
    MissingDependency {
        table: &'static str,
        row: usize,
    },
    ExtraDependency {
        row: usize,
    },
    WrongDependencyOwner {
        table: &'static str,
        row: usize,
    },
    OutOfScopeTypeParameter {
        table: &'static str,
        row: usize,
    },
}

impl std::fmt::Display for ProductValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid library declaration product: {self:?}")
    }
}

impl std::error::Error for ProductValidationError {}

impl LibraryDeclarationProduct {
    pub fn validate(&self) -> Result<(), ProductValidationError> {
        if self.profile.index() >= self.strings.len() || self.profile_identity().is_empty() {
            return Err(ProductValidationError::EmptyProfile);
        }
        if self.sources.is_empty() {
            return Err(ProductValidationError::EmptySources);
        }
        if self.containers.is_empty() {
            return Err(ProductValidationError::InvalidContainer {
                table: "containers",
                row: 0,
            });
        }
        for (row, container) in self.containers.iter().enumerate() {
            if container
                .parent
                .is_some_and(|key| key.index() >= self.containers.len())
                || container
                    .name
                    .is_some_and(|key| key.index() >= self.strings.len())
            {
                return Err(ProductValidationError::InvalidContainer {
                    table: "containers",
                    row,
                });
            }
            if let Some(anchor) = container.anchor {
                valid_site(self, anchor, "containers", row)?;
            }
            let shape_is_valid = match container.kind {
                crate::schema::ContainerKind::CompilationGlobal => {
                    container.parent.is_none()
                        && container.name.is_none()
                        && container.anchor.is_none()
                }
                crate::schema::ContainerKind::ExternalModule => {
                    container.parent.is_none()
                        && container.name.is_none()
                        && container.anchor.is_some()
                }
                crate::schema::ContainerKind::Namespace
                | crate::schema::ContainerKind::AmbientExternalModule => {
                    container.parent.is_some()
                        && container.name.is_some()
                        && container.anchor.is_some()
                }
            };
            if !shape_is_valid {
                return Err(ProductValidationError::InvalidContainer {
                    table: "containers",
                    row,
                });
            }
            let mut parent = container.parent;
            let mut depth = 0usize;
            while let Some(key) = parent {
                depth = depth.saturating_add(1);
                if depth > self.containers.len() {
                    return Err(ProductValidationError::InvalidContainer {
                        table: "containers",
                        row,
                    });
                }
                parent = self.containers[key.index()].parent;
            }
        }
        for (row, source) in self.sources.iter().enumerate() {
            if source.name.index() >= self.strings.len()
                || source.sha256.index() >= self.strings.len()
            {
                return Err(ProductValidationError::InvalidString {
                    table: "sources",
                    row,
                });
            }
            if source.root_region.index() >= self.regions.len() {
                return Err(ProductValidationError::InvalidRegion {
                    table: "sources",
                    row,
                });
            }
            let root = &self.regions[source.root_region.index()];
            let root_container = self.containers.get(root.container.index());
            if root.parent.is_some()
                || root.kind != crate::schema::RegionKind::Source
                || root.site.source.index() != row
                || root.site.start != 0
                || root.site.end != source.byte_len
                || root.container.index() >= self.containers.len()
                || !matches!(
                    (
                        source.classification,
                        root_container.map(|value| value.kind)
                    ),
                    (
                        crate::schema::SourceClassification::Script,
                        Some(crate::schema::ContainerKind::CompilationGlobal)
                    ) | (
                        crate::schema::SourceClassification::ExternalModule,
                        Some(crate::schema::ContainerKind::ExternalModule)
                    )
                )
            {
                return Err(ProductValidationError::InvalidRegion {
                    table: "sources",
                    row,
                });
            }
            if row > 0 && self.sources[row - 1].ordinal >= source.ordinal {
                return Err(ProductValidationError::NonCanonicalSourceOrder { row });
            }
        }
        for (row, container) in self.containers.iter().enumerate() {
            if container.kind != crate::schema::ContainerKind::ExternalModule {
                continue;
            }
            let Some(anchor) = container.anchor else {
                return Err(ProductValidationError::InvalidContainer {
                    table: "containers",
                    row,
                });
            };
            let Some(source) = self.sources.get(anchor.source.index()) else {
                return Err(ProductValidationError::InvalidContainer {
                    table: "containers",
                    row,
                });
            };
            if source.classification != crate::schema::SourceClassification::ExternalModule
                || anchor.start != 0
                || anchor.end != source.byte_len
                || self.regions[source.root_region.index()].container.index() != row
                || self
                    .containers
                    .iter()
                    .enumerate()
                    .any(|(other_row, other)| {
                        other_row != row
                            && other.kind == crate::schema::ContainerKind::ExternalModule
                            && other
                                .anchor
                                .is_some_and(|other_anchor| other_anchor.source == anchor.source)
                    })
            {
                return Err(ProductValidationError::InvalidContainer {
                    table: "containers",
                    row,
                });
            }
        }
        for (row, region) in self.regions.iter().enumerate() {
            valid_site(self, region.site, "regions", row)?;
            if region.container.index() >= self.containers.len() {
                return Err(ProductValidationError::InvalidContainer {
                    table: "regions",
                    row,
                });
            }
            let container_kind = self.containers[region.container.index()].kind;
            if !matches!(
                (region.kind, container_kind),
                (
                    crate::schema::RegionKind::Source,
                    crate::schema::ContainerKind::CompilationGlobal
                        | crate::schema::ContainerKind::ExternalModule
                ) | (
                    crate::schema::RegionKind::Namespace,
                    crate::schema::ContainerKind::Namespace
                ) | (
                    crate::schema::RegionKind::AmbientExternalModule,
                    crate::schema::ContainerKind::AmbientExternalModule
                ) | (
                    crate::schema::RegionKind::GlobalAugmentation,
                    crate::schema::ContainerKind::CompilationGlobal
                )
            ) {
                return Err(ProductValidationError::InvalidContainer {
                    table: "regions",
                    row,
                });
            }
            if region
                .parent
                .is_some_and(|key| key.index() >= self.regions.len())
            {
                return Err(ProductValidationError::InvalidRegion {
                    table: "regions",
                    row,
                });
            }
            let mut parent = region.parent;
            let mut depth = 0usize;
            while let Some(key) = parent {
                depth = depth.saturating_add(1);
                if depth > self.regions.len() {
                    return Err(ProductValidationError::InvalidRegionContainment { row });
                }
                let owner = &self.regions[key.index()];
                if owner.site.source != region.site.source
                    || owner.site.start > region.site.start
                    || region.site.end > owner.site.end
                {
                    return Err(ProductValidationError::InvalidRegionContainment { row });
                }
                parent = owner.parent;
            }
        }
        for (row, declaration) in self.declarations.iter().enumerate() {
            valid_site(self, declaration.site, "declarations", row)?;
            valid_site(self, declaration.binding_site, "declarations", row)?;
            if declaration.region.index() >= self.regions.len() {
                return Err(ProductValidationError::InvalidRegion {
                    table: "declarations",
                    row,
                });
            }
            let region = &self.regions[declaration.region.index()];
            if declaration.site.source != region.site.source
                || declaration.binding_site.source != declaration.site.source
                || declaration.site.start < region.site.start
                || region.site.end < declaration.site.end
                || declaration.binding_site.start < declaration.site.start
                || declaration.site.end < declaration.binding_site.end
            {
                return Err(ProductValidationError::InvalidRegionContainment { row });
            }
            if declaration.name.index() >= self.strings.len() {
                return Err(ProductValidationError::InvalidString {
                    table: "declarations",
                    row,
                });
            }
            match &declaration.payload {
                DeclarationPayload::Variable { annotation }
                | DeclarationPayload::Parameter { annotation }
                | DeclarationPayload::CatchParameter { annotation } => {
                    valid_optional_type(self, *annotation, "declarations", row)?
                }
                DeclarationPayload::Function { signature }
                    if signature.index() >= self.signatures.len() =>
                {
                    return Err(ProductValidationError::InvalidSignature { row })
                }
                DeclarationPayload::Class {
                    type_parameters,
                    members,
                    heritage,
                }
                | DeclarationPayload::Interface {
                    type_parameters,
                    members,
                    heritage,
                } if type_parameters
                    .iter()
                    .any(|key| key.index() >= self.type_parameters.len())
                    || members.iter().any(|key| key.index() >= self.members.len())
                    || heritage.iter().any(|key| key.index() >= self.types.len()) =>
                {
                    return Err(ProductValidationError::InvalidDeclaration {
                        table: "declarations",
                        row,
                    });
                }
                DeclarationPayload::TypeAlias {
                    type_parameters,
                    value,
                } if type_parameters
                    .iter()
                    .any(|key| key.index() >= self.type_parameters.len())
                    || value.index() >= self.types.len() =>
                {
                    return Err(ProductValidationError::InvalidDeclaration {
                        table: "declarations",
                        row,
                    });
                }
                DeclarationPayload::Enum { members }
                    if members.iter().any(|key| key.index() >= self.members.len()) =>
                {
                    return Err(ProductValidationError::InvalidDeclaration {
                        table: "declarations",
                        row,
                    })
                }
                DeclarationPayload::Namespace { child_region }
                | DeclarationPayload::Global { child_region }
                    if child_region.index() >= self.regions.len() =>
                {
                    return Err(ProductValidationError::InvalidRegion {
                        table: "declarations",
                        row,
                    })
                }
                _ => {}
            }
            if let DeclarationPayload::Class {
                members, heritage, ..
            }
            | DeclarationPayload::Interface {
                members, heritage, ..
            } = &declaration.payload
            {
                if members.iter().any(|key| {
                    let site = self.members[key.index()].site;
                    site.source != declaration.site.source
                        || site.start < declaration.site.start
                        || declaration.site.end < site.end
                }) || heritage.iter().any(|key| {
                    let site = self.types[key.index()].site;
                    site.source != declaration.site.source
                        || site.start < declaration.site.start
                        || declaration.site.end < site.end
                }) {
                    return Err(ProductValidationError::InvalidDeclaration {
                        table: "declarations",
                        row,
                    });
                }
            }
            match &declaration.payload {
                DeclarationPayload::Namespace { child_region } => {
                    valid_child_region(
                        self,
                        row,
                        declaration,
                        *child_region,
                        &[
                            crate::schema::RegionKind::Namespace,
                            crate::schema::RegionKind::AmbientExternalModule,
                        ],
                    )?;
                }
                DeclarationPayload::Global { child_region } => {
                    valid_child_region(
                        self,
                        row,
                        declaration,
                        *child_region,
                        &[crate::schema::RegionKind::GlobalAugmentation],
                    )?;
                }
                DeclarationPayload::Variable { annotation: _ }
                | DeclarationPayload::Function { signature: _ }
                | DeclarationPayload::Class { .. }
                | DeclarationPayload::Interface { .. }
                | DeclarationPayload::TypeAlias { .. }
                | DeclarationPayload::Enum { members: _ }
                | DeclarationPayload::Import
                | DeclarationPayload::ImportEquals
                | DeclarationPayload::NamespaceExport
                | DeclarationPayload::Parameter { annotation: _ }
                | DeclarationPayload::CatchParameter { annotation: _ }
                | DeclarationPayload::DefaultExportExpression => {}
            }
        }
        for (row, group) in self.groups.iter().enumerate() {
            if group.container.index() >= self.containers.len() {
                return Err(ProductValidationError::InvalidContainer {
                    table: "groups",
                    row,
                });
            }
            if group.name.index() >= self.strings.len() {
                return Err(ProductValidationError::InvalidString {
                    table: "groups",
                    row,
                });
            }
            let mut previous = None;
            let mut expected_slots = crate::schema::DeclarationSlots::default();
            for fragment in &group.fragments {
                if fragment.index() >= self.declarations.len() {
                    return Err(ProductValidationError::InvalidDeclaration {
                        table: "groups",
                        row,
                    });
                }
                if previous.is_some_and(|key| key >= *fragment) {
                    return Err(ProductValidationError::NonCanonicalFragmentOrder { row });
                }
                let declaration = &self.declarations[fragment.index()];
                if declaration.name != group.name {
                    return Err(ProductValidationError::GroupFragmentName {
                        group: row,
                        declaration: fragment.index(),
                    });
                }
                if self.regions[declaration.region.index()].container != group.container {
                    return Err(ProductValidationError::GroupFragmentContainer {
                        group: row,
                        declaration: fragment.index(),
                    });
                }
                expected_slots.value |= declaration.spaces.value;
                expected_slots.r#type |= declaration.spaces.r#type;
                expected_slots.namespace |= declaration.spaces.namespace;
                previous = Some(*fragment);
            }
            if group.slots != expected_slots {
                return Err(ProductValidationError::InvalidGroup {
                    table: "groups",
                    row,
                });
            }
        }
        for (row, attachment) in self.namespaces.iter().enumerate() {
            valid_attachment(
                self,
                attachment.group,
                attachment.declaration,
                "namespaces",
                row,
            )?;
            valid_attachment_shape(
                self,
                attachment.group,
                attachment.declaration,
                true,
                row,
                "namespaces",
            )?;
            if self.namespaces[..row].contains(attachment) {
                return Err(ProductValidationError::DuplicateAttachment {
                    table: "namespaces",
                    row,
                });
            }
        }
        for (row, attachment) in self.values.iter().enumerate() {
            valid_attachment(
                self,
                attachment.group,
                attachment.declaration,
                "values",
                row,
            )?;
            valid_attachment_shape(
                self,
                attachment.group,
                attachment.declaration,
                false,
                row,
                "values",
            )?;
            if self.values[..row].contains(attachment) {
                return Err(ProductValidationError::DuplicateAttachment {
                    table: "values",
                    row,
                });
            }
        }
        for (group_index, group) in self.groups.iter().enumerate() {
            for declaration in &group.fragments {
                let spaces = self.declarations[declaration.index()].spaces;
                if spaces.namespace
                    && !self.namespaces.iter().any(|attachment| {
                        attachment.group.index() == group_index
                            && attachment.declaration == *declaration
                    })
                {
                    return Err(ProductValidationError::MissingAttachment {
                        table: "namespaces",
                        group: group_index,
                        declaration: declaration.index(),
                    });
                }
                if spaces.value
                    && !self.values.iter().any(|attachment| {
                        attachment.group.index() == group_index
                            && attachment.declaration == *declaration
                    })
                {
                    return Err(ProductValidationError::MissingAttachment {
                        table: "values",
                        group: group_index,
                        declaration: declaration.index(),
                    });
                }
            }
        }
        for (row, parameter) in self.type_parameters.iter().enumerate() {
            valid_string(self, parameter.name, "type_parameters", row)?;
            valid_site(self, parameter.site, "type_parameters", row)?;
            valid_site(self, parameter.owner, "type_parameters", row)?;
            if parameter.site.source != parameter.owner.source
                || parameter.site.start < parameter.owner.start
                || parameter.owner.end < parameter.site.end
            {
                return Err(ProductValidationError::OutOfScopeTypeParameter {
                    table: "type_parameters",
                    row,
                });
            }
            for visibility in &parameter.visibility {
                valid_site(self, *visibility, "type_parameters", row)?;
                if visibility.source != parameter.owner.source
                    || visibility.start < parameter.owner.start
                    || parameter.owner.end < visibility.end
                {
                    return Err(ProductValidationError::OutOfScopeTypeParameter {
                        table: "type_parameters",
                        row,
                    });
                }
            }
            valid_optional_type(self, parameter.constraint, "type_parameters", row)?;
            valid_optional_type(self, parameter.default, "type_parameters", row)?;
        }
        for (row, recipe) in self.types.iter().enumerate() {
            valid_site(self, recipe.site, "types", row)?;
            for child in recipe_children(&recipe.node) {
                if child.index() >= self.types.len() {
                    return Err(ProductValidationError::InvalidTypeRecipe {
                        table: "types",
                        row,
                    });
                }
            }
            match &recipe.node {
                TypeRecipeNode::Function(key) | TypeRecipeNode::Constructor(key)
                    if key.index() >= self.signatures.len() =>
                {
                    return Err(ProductValidationError::InvalidSignature { row })
                }
                TypeRecipeNode::Object(keys)
                    if keys.iter().any(|key| key.index() >= self.members.len()) =>
                {
                    return Err(ProductValidationError::InvalidMember { row })
                }
                TypeRecipeNode::Mapped { parameter, .. } | TypeRecipeNode::Infer(parameter)
                    if parameter.index() >= self.type_parameters.len() =>
                {
                    return Err(ProductValidationError::InvalidTypeRecipe {
                        table: "types",
                        row,
                    })
                }
                TypeRecipeNode::Reference {
                    resolution,
                    dependency: _,
                    ..
                } if match resolution {
                    TypeReferenceResolution::Group(key) => key.index() >= self.groups.len(),
                    TypeReferenceResolution::TypeParameter(key) => {
                        key.index() >= self.type_parameters.len()
                    }
                    TypeReferenceResolution::Unresolved { region } => {
                        region.index() >= self.regions.len()
                    }
                } =>
                {
                    return Err(ProductValidationError::InvalidTypeRecipe {
                        table: "types",
                        row,
                    })
                }
                TypeRecipeNode::Query {
                    resolution,
                    dependency: _,
                    ..
                } if match resolution {
                    ValueReferenceResolution::Group(key) => key.index() >= self.groups.len(),
                    ValueReferenceResolution::IntrinsicGlobalValue => false,
                    ValueReferenceResolution::Unresolved { region } => {
                        region.index() >= self.regions.len()
                    }
                } =>
                {
                    return Err(ProductValidationError::InvalidTypeRecipe {
                        table: "types",
                        row,
                    })
                }
                _ => {}
            }
            if let TypeRecipeNode::Reference {
                resolution: TypeReferenceResolution::TypeParameter(parameter),
                dependency: _,
                ..
            } = &recipe.node
            {
                valid_type_parameter_use(self, *parameter, recipe.site, "types", row)?;
            }
            valid_type_strings(self, &recipe.node, row)?;
        }
        for (row, signature) in self.signatures.iter().enumerate() {
            if signature
                .result
                .is_some_and(|key| key.index() >= self.types.len())
                || signature.parameters.iter().any(|parameter| {
                    parameter
                        .value
                        .is_some_and(|key| key.index() >= self.types.len())
                })
                || signature
                    .type_parameters
                    .iter()
                    .any(|key| key.index() >= self.type_parameters.len())
                || signature
                    .parameters
                    .iter()
                    .any(|parameter| !valid_parameter_pattern_strings(self, &parameter.pattern))
            {
                return Err(ProductValidationError::InvalidSignature { row });
            }
        }
        for (row, member) in self.members.iter().enumerate() {
            valid_site(self, member.site, "members", row)?;
            if member
                .value
                .is_some_and(|key| key.index() >= self.types.len())
                || member
                    .signature
                    .is_some_and(|key| key.index() >= self.signatures.len())
                || !valid_property_name(self, member.name.as_ref())
            {
                return Err(ProductValidationError::InvalidMember { row });
            }
            if let Some(PropertyNameRecipe::Computed { site, value }) = &member.name {
                valid_site(self, *site, "members", row)?;
                if site.source != member.site.source
                    || site.start < member.site.start
                    || member.site.end < site.end
                    || !valid_value_name_resolution(self, value)
                {
                    return Err(ProductValidationError::InvalidMember { row });
                }
            }
        }
        for (row, dependency) in self.dependencies.iter().enumerate() {
            valid_site(self, dependency.site, "dependencies", row)?;
            if dependency.owner.index() >= self.groups.len()
                || matches!(&dependency.target, DependencyTarget::Group(key) if key.index() >= self.groups.len())
                || matches!(&dependency.target, DependencyTarget::TypeParameter(key) if key.index() >= self.type_parameters.len())
            {
                return Err(ProductValidationError::InvalidGroup {
                    table: "dependencies",
                    row,
                });
            }
            if let DependencyTarget::Unresolved { region, path } = &dependency.target {
                if region.index() >= self.regions.len() {
                    return Err(ProductValidationError::InvalidRegion {
                        table: "dependencies",
                        row,
                    });
                }
                if path.is_empty() {
                    return Err(ProductValidationError::InvalidTypeRecipe {
                        table: "dependencies",
                        row,
                    });
                }
                for key in path {
                    valid_string(self, *key, "dependencies", row)?;
                }
            }
            let owner = &self.groups[dependency.owner.index()];
            if !owner.fragments.iter().any(|declaration| {
                let site = self.declarations[declaration.index()].site;
                site.source == dependency.site.source
                    && site.start <= dependency.site.start
                    && dependency.site.end <= site.end
            }) {
                return Err(ProductValidationError::WrongDependencyOwner {
                    table: "dependencies",
                    row,
                });
            }
            if let DependencyTarget::TypeParameter(parameter) = &dependency.target {
                valid_type_parameter_use(self, *parameter, dependency.site, "dependencies", row)?;
            }
        }
        validate_exact_dependency_multiset(self)?;
        for (row, step) in self.lexical_steps.iter().enumerate() {
            valid_site(self, step.site, "lexical_steps", row)?;
            if step.owner.index() >= self.declarations.len() {
                return Err(ProductValidationError::InvalidDeclaration {
                    table: "lexical_steps",
                    row,
                });
            }
            let owner = &self.declarations[step.owner.index()];
            if step.site.source != owner.site.source
                || step.site.start < owner.site.start
                || owner.site.end < step.site.end
            {
                return Err(ProductValidationError::InvalidLexicalStep { row });
            }
        }
        Ok(())
    }
}

fn valid_child_region(
    product: &LibraryDeclarationProduct,
    declaration_row: usize,
    declaration: &crate::schema::DeclarationRecipe,
    child_region: crate::keys::RegionKey,
    expected_kinds: &[crate::schema::RegionKind],
) -> Result<(), ProductValidationError> {
    let child = &product.regions[child_region.index()];
    let parent_container = product.regions[declaration.region.index()].container;
    let child_container = &product.containers[child.container.index()];
    let container_parent_is_valid = match child.kind {
        crate::schema::RegionKind::GlobalAugmentation => {
            child.container == parent_container
                || child_container.kind == crate::schema::ContainerKind::CompilationGlobal
        }
        crate::schema::RegionKind::Namespace | crate::schema::RegionKind::AmbientExternalModule => {
            child_container.parent == Some(parent_container)
        }
        crate::schema::RegionKind::Source => false,
    };
    if child.parent != Some(declaration.region)
        || !expected_kinds.contains(&child.kind)
        || !container_parent_is_valid
        || child.site.source != declaration.site.source
        || child.site.start < declaration.site.start
        || declaration.site.end < child.site.end
    {
        return Err(ProductValidationError::InvalidChildRegion {
            declaration: declaration_row,
        });
    }
    Ok(())
}

fn valid_type_parameter_use(
    product: &LibraryDeclarationProduct,
    parameter: crate::keys::TypeParameterKey,
    site: crate::schema::SourceSite,
    table: &'static str,
    row: usize,
) -> Result<(), ProductValidationError> {
    let Some(parameter) = product.type_parameters.get(parameter.index()) else {
        return Err(ProductValidationError::OutOfScopeTypeParameter { table, row });
    };
    if !parameter.visibility.iter().any(|visibility| {
        visibility.source == site.source
            && visibility.start <= site.start
            && site.end <= visibility.end
    }) {
        return Err(ProductValidationError::OutOfScopeTypeParameter { table, row });
    }
    Ok(())
}

fn valid_value_name_resolution(
    product: &LibraryDeclarationProduct,
    value: &SymbolicValueNameRecipe,
) -> bool {
    match value {
        SymbolicValueNameRecipe::Path {
            path: _,
            resolution,
            dependency: _,
        } => match resolution {
            ValueReferenceResolution::Group(group) => group.index() < product.groups.len(),
            ValueReferenceResolution::IntrinsicGlobalValue => true,
            ValueReferenceResolution::Unresolved { region } => {
                region.index() < product.regions.len()
            }
        },
        SymbolicValueNameRecipe::String(_text) => true,
        SymbolicValueNameRecipe::NumberBits(_number) => true,
    }
}

struct ExpectedDependency {
    table: &'static str,
    row: usize,
    recipe: crate::schema::DependencyRecipe,
}

fn validate_exact_dependency_multiset(
    product: &LibraryDeclarationProduct,
) -> Result<(), ProductValidationError> {
    let expected = expected_dependencies(product)?;
    let mut matched = vec![false; product.dependencies.len()];
    for expected_row in expected {
        if let Some((row, _)) = product
            .dependencies
            .iter()
            .enumerate()
            .find(|(row, actual)| !matched[*row] && *actual == &expected_row.recipe)
        {
            matched[row] = true;
            continue;
        }
        if let Some((row, _)) = product
            .dependencies
            .iter()
            .enumerate()
            .find(|(row, actual)| {
                !matched[*row]
                    && actual.site == expected_row.recipe.site
                    && actual.kind == expected_row.recipe.kind
                    && actual.target == expected_row.recipe.target
            })
        {
            return Err(ProductValidationError::WrongDependencyOwner {
                table: "dependencies",
                row,
            });
        }
        return Err(ProductValidationError::MissingDependency {
            table: expected_row.table,
            row: expected_row.row,
        });
    }
    if let Some(row) = matched.iter().position(|matched| !matched) {
        return Err(ProductValidationError::ExtraDependency { row });
    }
    Ok(())
}

fn expected_dependencies(
    product: &LibraryDeclarationProduct,
) -> Result<Vec<ExpectedDependency>, ProductValidationError> {
    let mut expected = Vec::new();
    for (row, recipe) in product.types.iter().enumerate() {
        match &recipe.node {
            TypeRecipeNode::Reference {
                path,
                resolution,
                arguments: _,
                dependency,
            } => {
                if !matches!(
                    dependency.kind,
                    DependencyKind::Reference | DependencyKind::Heritage
                ) {
                    return Err(ProductValidationError::DependencyReferenceMismatch { row });
                }
                valid_dependency_owner(product, *dependency, recipe.site, "types", row)?;
                expected.push(ExpectedDependency {
                    table: "types",
                    row,
                    recipe: crate::schema::DependencyRecipe {
                        owner: dependency.owner,
                        target: dependency_target_for_type(resolution, path),
                        kind: dependency.kind,
                        site: recipe.site,
                    },
                });
            }
            TypeRecipeNode::Query {
                path,
                resolution,
                dependency,
            } => {
                if dependency.kind != DependencyKind::Query {
                    return Err(ProductValidationError::DependencyReferenceMismatch { row });
                }
                valid_dependency_owner(product, *dependency, recipe.site, "types", row)?;
                expected.push(ExpectedDependency {
                    table: "types",
                    row,
                    recipe: crate::schema::DependencyRecipe {
                        owner: dependency.owner,
                        target: dependency_target_for_value(resolution, path),
                        kind: dependency.kind,
                        site: recipe.site,
                    },
                });
            }
            TypeRecipeNode::Keyword(_)
            | TypeRecipeNode::Literal(_)
            | TypeRecipeNode::Import { .. }
            | TypeRecipeNode::Array(_)
            | TypeRecipeNode::Tuple { .. }
            | TypeRecipeNode::Union(_)
            | TypeRecipeNode::Intersection(_)
            | TypeRecipeNode::Function(_)
            | TypeRecipeNode::Constructor(_)
            | TypeRecipeNode::Object(_)
            | TypeRecipeNode::Operator { .. }
            | TypeRecipeNode::IndexedAccess { .. }
            | TypeRecipeNode::Mapped { .. }
            | TypeRecipeNode::Conditional { .. }
            | TypeRecipeNode::Infer(_)
            | TypeRecipeNode::Template { .. }
            | TypeRecipeNode::Predicate { .. } => {}
        }
    }
    for (row, member) in product.members.iter().enumerate() {
        let Some(PropertyNameRecipe::Computed { site, value }) = &member.name else {
            continue;
        };
        let SymbolicValueNameRecipe::Path {
            path,
            resolution,
            dependency,
        } = value
        else {
            continue;
        };
        if dependency.kind != DependencyKind::Query {
            return Err(ProductValidationError::DependencyReferenceMismatch { row });
        }
        valid_dependency_owner(product, *dependency, *site, "members", row)?;
        expected.push(ExpectedDependency {
            table: "members",
            row,
            recipe: crate::schema::DependencyRecipe {
                owner: dependency.owner,
                target: dependency_target_for_value(resolution, path),
                kind: dependency.kind,
                site: *site,
            },
        });
    }
    Ok(expected)
}

fn valid_dependency_owner(
    product: &LibraryDeclarationProduct,
    dependency: crate::schema::DependencyLinkRecipe,
    site: crate::schema::SourceSite,
    table: &'static str,
    row: usize,
) -> Result<(), ProductValidationError> {
    let Some(group) = product.groups.get(dependency.owner.index()) else {
        return Err(ProductValidationError::WrongDependencyOwner { table, row });
    };
    if !group.fragments.iter().any(|declaration| {
        let declaration_site = product.declarations[declaration.index()].site;
        declaration_site.source == site.source
            && declaration_site.start <= site.start
            && site.end <= declaration_site.end
    }) {
        return Err(ProductValidationError::WrongDependencyOwner { table, row });
    }
    Ok(())
}

fn dependency_target_for_type(
    resolution: &TypeReferenceResolution,
    path: &[crate::keys::StringKey],
) -> DependencyTarget {
    match resolution {
        TypeReferenceResolution::Group(group) => DependencyTarget::Group(*group),
        TypeReferenceResolution::TypeParameter(parameter) => {
            DependencyTarget::TypeParameter(*parameter)
        }
        TypeReferenceResolution::Unresolved { region } => DependencyTarget::Unresolved {
            region: *region,
            path: path.to_vec().into_boxed_slice(),
        },
    }
}

fn dependency_target_for_value(
    resolution: &ValueReferenceResolution,
    path: &[crate::keys::StringKey],
) -> DependencyTarget {
    match resolution {
        ValueReferenceResolution::Group(group) => DependencyTarget::Group(*group),
        ValueReferenceResolution::IntrinsicGlobalValue => DependencyTarget::IntrinsicGlobalValue,
        ValueReferenceResolution::Unresolved { region } => DependencyTarget::Unresolved {
            region: *region,
            path: path.to_vec().into_boxed_slice(),
        },
    }
}

fn valid_attachment(
    product: &LibraryDeclarationProduct,
    group: crate::keys::GroupKey,
    declaration: crate::keys::DeclarationKey,
    table: &'static str,
    row: usize,
) -> Result<(), ProductValidationError> {
    if group.index() >= product.groups.len() {
        return Err(ProductValidationError::InvalidGroup { table, row });
    }
    if declaration.index() >= product.declarations.len() {
        return Err(ProductValidationError::InvalidDeclaration { table, row });
    }
    Ok(())
}

fn valid_attachment_shape(
    product: &LibraryDeclarationProduct,
    group: crate::keys::GroupKey,
    declaration: crate::keys::DeclarationKey,
    namespace: bool,
    row: usize,
    table: &'static str,
) -> Result<(), ProductValidationError> {
    let group = &product.groups[group.index()];
    let declaration_spaces = product.declarations[declaration.index()].spaces;
    if !group.fragments.contains(&declaration)
        || (namespace && !group.slots.namespace)
        || (!namespace && !group.slots.value)
        || (namespace && !declaration_spaces.namespace)
        || (!namespace && !declaration_spaces.value)
    {
        return Err(ProductValidationError::ExtraAttachment { table, row });
    }
    Ok(())
}

fn valid_string(
    product: &LibraryDeclarationProduct,
    key: crate::keys::StringKey,
    table: &'static str,
    row: usize,
) -> Result<(), ProductValidationError> {
    if key.index() >= product.strings.len() {
        return Err(ProductValidationError::InvalidString { table, row });
    }
    Ok(())
}

fn valid_type_strings(
    product: &LibraryDeclarationProduct,
    node: &TypeRecipeNode,
    row: usize,
) -> Result<(), ProductValidationError> {
    let keys: Vec<_> = match node {
        TypeRecipeNode::Literal(LiteralRecipe::String(key) | LiteralRecipe::BigInt(key)) => {
            vec![*key]
        }
        TypeRecipeNode::Reference { path, .. } | TypeRecipeNode::Query { path, .. } => {
            path.to_vec()
        }
        TypeRecipeNode::Import {
            argument,
            qualifier,
            ..
        } => std::iter::once(*argument)
            .chain(qualifier.iter().copied())
            .collect(),
        TypeRecipeNode::Tuple { elements } => {
            elements.iter().filter_map(|element| element.name).collect()
        }
        TypeRecipeNode::Template { texts, .. } => texts.to_vec(),
        TypeRecipeNode::Predicate { parameter, .. } => vec![*parameter],
        TypeRecipeNode::Keyword(_)
        | TypeRecipeNode::Literal(LiteralRecipe::NumberBits(_) | LiteralRecipe::Boolean(_))
        | TypeRecipeNode::Array(_)
        | TypeRecipeNode::Union(_)
        | TypeRecipeNode::Intersection(_)
        | TypeRecipeNode::Function(_)
        | TypeRecipeNode::Constructor(_)
        | TypeRecipeNode::Object(_)
        | TypeRecipeNode::Operator { .. }
        | TypeRecipeNode::IndexedAccess { .. }
        | TypeRecipeNode::Mapped { .. }
        | TypeRecipeNode::Conditional { .. }
        | TypeRecipeNode::Infer(_) => Vec::new(),
    };
    for key in keys {
        valid_string(product, key, "types", row)?;
    }
    Ok(())
}

fn valid_parameter_pattern_strings(
    product: &LibraryDeclarationProduct,
    pattern: &ParameterPattern,
) -> bool {
    match pattern {
        ParameterPattern::Identifier(key) => key.index() < product.strings.len(),
        ParameterPattern::Array(keys) | ParameterPattern::Object(keys) => {
            keys.iter().all(|key| key.index() < product.strings.len())
        }
    }
}

fn valid_property_name(
    product: &LibraryDeclarationProduct,
    name: Option<&PropertyNameRecipe>,
) -> bool {
    match name {
        Some(
            PropertyNameRecipe::Identifier(key)
            | PropertyNameRecipe::String(key)
            | PropertyNameRecipe::Private(key),
        ) => key.index() < product.strings.len(),
        Some(PropertyNameRecipe::Computed { site: _, value }) => match value {
            crate::schema::SymbolicValueNameRecipe::Path {
                path,
                resolution: _,
                dependency: _,
            } => !path.is_empty() && path.iter().all(|key| key.index() < product.strings.len()),
            crate::schema::SymbolicValueNameRecipe::String(key) => {
                key.index() < product.strings.len()
            }
            crate::schema::SymbolicValueNameRecipe::NumberBits(_) => true,
        },
        Some(PropertyNameRecipe::NumberBits(_)) | None => true,
    }
}

fn valid_optional_type(
    product: &LibraryDeclarationProduct,
    key: Option<crate::keys::TypeRecipeKey>,
    table: &'static str,
    row: usize,
) -> Result<(), ProductValidationError> {
    if key.is_some_and(|key| key.index() >= product.types.len()) {
        return Err(ProductValidationError::InvalidTypeRecipe { table, row });
    }
    Ok(())
}

fn valid_site(
    product: &LibraryDeclarationProduct,
    site: crate::schema::SourceSite,
    table: &'static str,
    row: usize,
) -> Result<(), ProductValidationError> {
    let Some(source) = product.sources.get(site.source.index()) else {
        return Err(ProductValidationError::InvalidSourceSite { table, row });
    };
    if site.start > site.end
        || usize::try_from(site.end).unwrap_or(usize::MAX)
            > usize::try_from(source.byte_len).unwrap_or(usize::MAX)
    {
        return Err(ProductValidationError::InvalidSourceSite { table, row });
    }
    Ok(())
}

fn recipe_children(node: &TypeRecipeNode) -> Vec<crate::keys::TypeRecipeKey> {
    match node {
        TypeRecipeNode::Keyword(_)
        | TypeRecipeNode::Literal(_)
        | TypeRecipeNode::Query { .. }
        | TypeRecipeNode::Infer(_) => Vec::new(),
        TypeRecipeNode::Reference { arguments, .. } | TypeRecipeNode::Import { arguments, .. } => {
            arguments.to_vec()
        }
        TypeRecipeNode::Array(child) | TypeRecipeNode::Operator { operand: child, .. } => {
            vec![*child]
        }
        TypeRecipeNode::Tuple { elements } => {
            elements.iter().map(|element| element.value).collect()
        }
        TypeRecipeNode::Union(children) | TypeRecipeNode::Intersection(children) => {
            children.to_vec()
        }
        TypeRecipeNode::Function(_)
        | TypeRecipeNode::Constructor(_)
        | TypeRecipeNode::Object(_) => Vec::new(),
        TypeRecipeNode::IndexedAccess { object, index } => vec![*object, *index],
        TypeRecipeNode::Mapped { name, value, .. } => {
            name.iter().chain(value.iter()).copied().collect()
        }
        TypeRecipeNode::Conditional {
            check,
            extends,
            when_true,
            when_false,
        } => vec![*check, *extends, *when_true, *when_false],
        TypeRecipeNode::Template { holes, .. } => holes.to_vec(),
        TypeRecipeNode::Predicate { value, .. } => value.iter().copied().collect(),
    }
}

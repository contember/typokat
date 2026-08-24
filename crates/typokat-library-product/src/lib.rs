mod canonical;
mod keys;
mod schema;
mod validate;
mod walk;

pub use canonical::{sha256_hex, ProductAuditReceipt};
pub use keys::*;
pub use schema::*;
pub use validate::ProductValidationError;
#[cfg(feature = "test-utils")]
pub use validate::{
    lexical_scope_corruption_receipt_for_test, with_changed_callable_site_for_test,
    LexicalScopeCorruptionReceipt,
};
pub use walk::{ProductFieldClass, ProductFieldVisitor};

#[derive(Default)]
pub struct ProductAssembly {
    strings: Vec<String>,
    sources: Vec<SourceRecipe>,
    containers: Vec<ContainerRecipe>,
    regions: Vec<RegionRecipe>,
    callable_scopes: Vec<CallableScopeRecipe>,
    declarations: Vec<DeclarationRecipe>,
    groups: Vec<DeclarationGroupRecipe>,
    namespaces: Vec<NamespaceAttachmentRecipe>,
    values: Vec<ValueAttachmentRecipe>,
    intrinsic_markers: Vec<IntrinsicMarkerRecipe>,
    type_parameters: Vec<TypeParameterRecipe>,
    types: Vec<TypeRecipe>,
    signatures: Vec<SignatureRecipe>,
    members: Vec<MemberRecipe>,
    dependencies: Vec<DependencyRecipe>,
    lexical_steps: Vec<LexicalStepRecipe>,
}

macro_rules! add_row {
    ($method:ident, $field:ident, $input:ty, $key:ty) => {
        pub fn $method(&mut self, value: $input) -> Result<$key, ProductBuildError> {
            let key = <$key>::new(self.$field.len())
                .ok_or(ProductBuildError::TooManyRows(stringify!($field)))?;
            self.$field.push(value);
            Ok(key)
        }
    };
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProductBuildError {
    TooManyRows(&'static str),
    Validation(ProductValidationError),
}

impl std::fmt::Display for ProductBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "library declaration product build failed: {self:?}"
        )
    }
}

impl std::error::Error for ProductBuildError {}

impl ProductAssembly {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn next_source_key(&self) -> Result<SourceKey, ProductBuildError> {
        SourceKey::new(self.sources.len()).ok_or(ProductBuildError::TooManyRows("sources"))
    }
    pub fn next_region_key(&self) -> Result<RegionKey, ProductBuildError> {
        RegionKey::new(self.regions.len()).ok_or(ProductBuildError::TooManyRows("regions"))
    }
    pub fn next_callable_scope_key(&self) -> Result<CallableScopeKey, ProductBuildError> {
        CallableScopeKey::new(self.callable_scopes.len())
            .ok_or(ProductBuildError::TooManyRows("callable_scopes"))
    }
    pub fn next_container_key(&self) -> Result<ContainerKey, ProductBuildError> {
        ContainerKey::new(self.containers.len()).ok_or(ProductBuildError::TooManyRows("containers"))
    }
    pub fn set_container_parent(
        &mut self,
        key: ContainerKey,
        parent: ContainerKey,
    ) -> Result<(), ProductBuildError> {
        let Some(container) = self.containers.get_mut(key.index()) else {
            return Err(ProductBuildError::TooManyRows("containers"));
        };
        container.parent = Some(parent);
        Ok(())
    }
    pub fn next_group_key(&self) -> Result<GroupKey, ProductBuildError> {
        GroupKey::new(self.groups.len()).ok_or(ProductBuildError::TooManyRows("groups"))
    }
    pub fn source_key_at(&self, index: usize) -> Option<SourceKey> {
        (index < self.sources.len())
            .then(|| SourceKey::new(index))
            .flatten()
    }
    pub fn member_offset(&self) -> Result<u32, ProductBuildError> {
        u32::try_from(self.members.len()).map_err(|_| ProductBuildError::TooManyRows("members"))
    }
    pub fn type_offset(&self) -> Result<u32, ProductBuildError> {
        u32::try_from(self.types.len()).map_err(|_| ProductBuildError::TooManyRows("types"))
    }
    pub fn type_parameter_offset(&self) -> Result<u32, ProductBuildError> {
        u32::try_from(self.type_parameters.len())
            .map_err(|_| ProductBuildError::TooManyRows("type_parameters"))
    }

    pub fn intern(&mut self, value: impl Into<String>) -> Result<StringKey, ProductBuildError> {
        let value = value.into();
        if let Some(index) = self.strings.iter().position(|existing| existing == &value) {
            return StringKey::new(index).ok_or(ProductBuildError::TooManyRows("strings"));
        }
        let key =
            StringKey::new(self.strings.len()).ok_or(ProductBuildError::TooManyRows("strings"))?;
        self.strings.push(value);
        Ok(key)
    }

    add_row!(add_source, sources, SourceRecipe, SourceKey);
    add_row!(add_container, containers, ContainerRecipe, ContainerKey);
    add_row!(add_region, regions, RegionRecipe, RegionKey);
    add_row!(
        add_callable_scope,
        callable_scopes,
        CallableScopeRecipe,
        CallableScopeKey
    );
    add_row!(
        add_declaration,
        declarations,
        DeclarationRecipe,
        DeclarationKey
    );
    add_row!(add_group, groups, DeclarationGroupRecipe, GroupKey);
    add_row!(
        add_namespace,
        namespaces,
        NamespaceAttachmentRecipe,
        NamespaceKey
    );
    add_row!(add_value, values, ValueAttachmentRecipe, ValueKey);
    add_row!(
        add_type_parameter,
        type_parameters,
        TypeParameterRecipe,
        TypeParameterKey
    );
    add_row!(add_type_recipe, types, TypeRecipe, TypeRecipeKey);
    add_row!(add_signature, signatures, SignatureRecipe, SignatureKey);
    add_row!(add_member, members, MemberRecipe, MemberKey);

    pub fn set_group_shape(
        &mut self,
        key: GroupKey,
        container: ContainerKey,
        slots: DeclarationSlots,
        fragments: Box<[DeclarationKey]>,
    ) -> Result<(), ProductBuildError> {
        let Some(group) = self.groups.get_mut(key.index()) else {
            return Err(ProductBuildError::TooManyRows("groups"));
        };
        group.container = container;
        group.slots = slots;
        group.fragments = fragments;
        Ok(())
    }

    pub fn add_dependency(&mut self, value: DependencyRecipe) {
        self.dependencies.push(value);
    }
    pub fn add_lexical_step(&mut self, value: LexicalStepRecipe) {
        self.lexical_steps.push(value);
    }
    pub fn add_intrinsic_marker(&mut self, value: IntrinsicMarkerRecipe) {
        self.intrinsic_markers.push(value);
    }

    pub fn finish(
        self,
        profile_identity: impl Into<String>,
    ) -> Result<LibraryDeclarationProduct, ProductBuildError> {
        let mut strings = self.strings;
        let profile_text = profile_identity.into();
        let profile = match strings.iter().position(|value| value == &profile_text) {
            Some(index) => {
                StringKey::new(index).ok_or(ProductBuildError::TooManyRows("strings"))?
            }
            None => {
                let key = StringKey::new(strings.len())
                    .ok_or(ProductBuildError::TooManyRows("strings"))?;
                strings.push(profile_text);
                key
            }
        };
        let product = LibraryDeclarationProduct {
            profile,
            strings: strings.into_boxed_slice(),
            sources: self.sources.into_boxed_slice(),
            containers: self.containers.into_boxed_slice(),
            regions: self.regions.into_boxed_slice(),
            callable_scopes: self.callable_scopes.into_boxed_slice(),
            declarations: self.declarations.into_boxed_slice(),
            groups: self.groups.into_boxed_slice(),
            namespaces: self.namespaces.into_boxed_slice(),
            values: self.values.into_boxed_slice(),
            intrinsic_markers: self.intrinsic_markers.into_boxed_slice(),
            type_parameters: self.type_parameters.into_boxed_slice(),
            types: self.types.into_boxed_slice(),
            signatures: self.signatures.into_boxed_slice(),
            members: self.members.into_boxed_slice(),
            dependencies: self.dependencies.into_boxed_slice(),
            lexical_steps: self.lexical_steps.into_boxed_slice(),
        };
        product.validate().map_err(ProductBuildError::Validation)?;
        Ok(product)
    }
}

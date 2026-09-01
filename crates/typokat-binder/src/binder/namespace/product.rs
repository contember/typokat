use super::deferred::{push_member, push_placement};
use super::metadata::WalkContext;
use super::models_ids::*;
use crate::binder::bind::BindState;
use crate::binder::declaration::{
    DeclarationKind, DeclarationSite, TypeFragmentKind, TypeGroupFragment,
};
use crate::binder::scope::{Scope, ScopeId, ScopeKind};
use crate::binder::symbol::Symbol;
use crate::source::{CompilationOrigin, LibraryFileOrdinal};
use crate::span::Span;
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;
use typokat_library_product as product;

#[cfg(test)]
thread_local! {
    static NAMESPACE_REGION_INDEX_PROBES: std::cell::Cell<u64> = const {
        std::cell::Cell::new(0)
    };
}

/// Dense index entries inspected while resolving declaration regions to namespace fragments.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct NamespaceRegionLookupWorkForTest {
    pub(crate) index_probes: u64,
}

#[cfg(test)]
fn namespace_region_lookup_work_for_test() -> NamespaceRegionLookupWorkForTest {
    NamespaceRegionLookupWorkForTest {
        index_probes: NAMESPACE_REGION_INDEX_PROBES.get(),
    }
}

#[cfg(test)]
fn record_namespace_region_index_probe() {
    NAMESPACE_REGION_INDEX_PROBES.set(NAMESPACE_REGION_INDEX_PROBES.get().saturating_add(1));
}

#[cfg(test)]
pub(crate) struct NamespaceRegionLookupWorkScopeForTest(NamespaceRegionLookupWorkForTest);

#[cfg(test)]
impl NamespaceRegionLookupWorkScopeForTest {
    pub(crate) fn start() -> Self {
        Self(namespace_region_lookup_work_for_test())
    }

    pub(crate) fn finish(self) -> NamespaceRegionLookupWorkForTest {
        let end = namespace_region_lookup_work_for_test();
        NamespaceRegionLookupWorkForTest {
            index_probes: end.index_probes.saturating_sub(self.0.index_probes),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryProductInstantiationError {
    InvalidProduct(product::ProductValidationError),
    MissingSource(usize),
    MissingRegion(usize),
    MissingContainer(usize),
    MissingDeclaration(usize),
    MissingGroup(usize),
    MissingString(usize),
    MissingNamespaceGroup(usize),
    MissingNamespaceContainer(usize),
    UnsupportedAmbientExternalModule(usize),
    UnsupportedDeclarationKind(product::DeclarationKind),
    MixedGroupLexicalScope(usize),
    DuplicateScopeDeclaration { scope: u32, name: String },
    NamespaceClassification(&'static str),
}

impl std::fmt::Display for LibraryProductInstantiationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "library product instantiation failed: {self:?}")
    }
}

impl std::error::Error for LibraryProductInstantiationError {}

pub struct LibraryProductInstantiationFacts {
    pub units: Vec<crate::binder::bind::LibraryBinderUnit>,
    pub final_module: ScopeId,
    pub fn_scopes: FxHashMap<(ScopeId, u32), ScopeId>,
    pub fn_decl_ids: FxHashMap<(ScopeId, u32), crate::binder::declaration::ValueStorageId>,
    pub map: LibraryProductBinderMap,
}

/// Project-local translation from immutable product keys to this binder's identities.
pub struct LibraryProductBinderMap {
    sources: Box<[LibraryProductSourceBinding]>,
    containers: Box<[ScopeId]>,
    regions: Box<[LibraryProductRegionBinding]>,
    callable_scopes: Box<[ScopeId]>,
    declarations: Box<[LibraryProductDeclarationBinding]>,
    groups: Box<[LibraryProductGroupBinding]>,
    group_locations: Box<[LibraryProductGroupLocation]>,
}

struct LibraryProductGroupLocation {
    scope: ScopeId,
    name: Box<str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LibraryProductSourceBinding {
    source: crate::binder::namespace::SourceUnitKey,
    module: ScopeId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LibraryProductRegionBinding {
    syntax_scope: ScopeId,
    lexical_scope: ScopeId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LibraryProductDeclarationBinding {
    declaration: crate::binder::declaration::DeclId,
    value: Option<crate::binder::declaration::ValueStorageId>,
    namespace_fragment: Option<crate::binder::namespace::NamespaceFragmentId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LibraryProductGroupBinding {
    symbol: crate::binder::symbol::SymbolId,
    ty: Option<crate::binder::declaration::TypeGroupId>,
    value: Option<crate::binder::declaration::ValueStorageId>,
    namespace: Option<crate::binder::namespace::NamespaceId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryProductBinderMapError {
    MissingGroup(usize),
}

impl std::fmt::Display for LibraryProductBinderMapError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "library product binder map finalization failed: {self:?}"
        )
    }
}

impl std::error::Error for LibraryProductBinderMapError {}

impl LibraryProductBinderMap {
    pub fn source(&self, key: product::SourceKey) -> Option<LibraryProductSourceBinding> {
        self.sources.get(key.index()).copied()
    }

    pub fn container(&self, key: product::ContainerKey) -> Option<ScopeId> {
        self.containers.get(key.index()).copied()
    }

    pub fn region(&self, key: product::RegionKey) -> Option<LibraryProductRegionBinding> {
        self.regions.get(key.index()).copied()
    }

    pub fn callable_scope(&self, key: product::CallableScopeKey) -> Option<ScopeId> {
        self.callable_scopes.get(key.index()).copied()
    }

    pub fn declaration(
        &self,
        key: product::DeclarationKey,
    ) -> Option<LibraryProductDeclarationBinding> {
        self.declarations.get(key.index()).copied()
    }

    pub fn group(&self, key: product::GroupKey) -> Option<LibraryProductGroupBinding> {
        self.groups.get(key.index()).copied()
    }

    pub(crate) fn finalize(
        &mut self,
        binder: &crate::binder::bind::Binder,
    ) -> Result<(), LibraryProductBinderMapError> {
        for (index, (group, location)) in self
            .groups
            .iter_mut()
            .zip(&self.group_locations)
            .enumerate()
        {
            let final_symbol = binder
                .graph
                .get(location.scope)
                .and_then(|scope| scope.lookup_local(&location.name))
                .ok_or(LibraryProductBinderMapError::MissingGroup(index))?;
            let symbol = binder
                .symbols
                .get(final_symbol)
                .ok_or(LibraryProductBinderMapError::MissingGroup(index))?;
            group.symbol = final_symbol;
            group.ty = symbol.ty;
            group.value = symbol.value;
            group.namespace = symbol.ns;
        }
        Ok(())
    }
}

impl LibraryProductSourceBinding {
    pub fn source(self) -> crate::binder::namespace::SourceUnitKey {
        self.source
    }

    pub fn module(self) -> ScopeId {
        self.module
    }
}

impl LibraryProductRegionBinding {
    pub fn syntax_scope(self) -> ScopeId {
        self.syntax_scope
    }

    pub fn lexical_scope(self) -> ScopeId {
        self.lexical_scope
    }
}

impl LibraryProductDeclarationBinding {
    pub fn declaration(self) -> crate::binder::declaration::DeclId {
        self.declaration
    }

    pub fn value(self) -> Option<crate::binder::declaration::ValueStorageId> {
        self.value
    }

    pub fn namespace_fragment(self) -> Option<crate::binder::namespace::NamespaceFragmentId> {
        self.namespace_fragment
    }
}

impl LibraryProductGroupBinding {
    pub fn symbol(self) -> crate::binder::symbol::SymbolId {
        self.symbol
    }

    pub fn ty(self) -> Option<crate::binder::declaration::TypeGroupId> {
        self.ty
    }

    pub fn value(self) -> Option<crate::binder::declaration::ValueStorageId> {
        self.value
    }

    pub fn namespace(self) -> Option<crate::binder::namespace::NamespaceId> {
        self.namespace
    }
}

#[allow(clippy::too_many_lines)]
pub(in crate::binder) fn instantiate_library_declaration_product(
    state: &mut BindState,
    declaration_product: &product::LibraryDeclarationProduct,
    compilation_global: ScopeId,
    script_namespace_root: ScopeId,
) -> Result<LibraryProductInstantiationFacts, LibraryProductInstantiationError> {
    declaration_product
        .validate()
        .map_err(LibraryProductInstantiationError::InvalidProduct)?;
    let strings = declaration_product.strings();
    let string = |key: product::StringKey| {
        strings
            .get(key.index())
            .map(String::as_str)
            .ok_or(LibraryProductInstantiationError::MissingString(key.index()))
    };

    let mut source_modules = Vec::with_capacity(declaration_product.sources().len());
    let mut source_keys = Vec::with_capacity(declaration_product.sources().len());
    let mut units = Vec::with_capacity(declaration_product.sources().len());
    for (source_index, source) in declaration_product.sources().iter().enumerate() {
        let module = state
            .graph
            .push(Scope::new(ScopeKind::Module, Some(script_namespace_root)));
        let source_key =
            state
                .fresh_source_key()
                .ok_or(LibraryProductInstantiationError::MissingSource(
                    source_index,
                ))?;
        let ordinal =
            LibraryFileOrdinal::new(usize::try_from(source.ordinal).map_err(|_| {
                LibraryProductInstantiationError::MissingSource(source_modules.len())
            })?);
        let context = ModuleBindingContext {
            source_file_kind: SourceFileKind::DeclarationTs,
            external_module: matches!(
                source.classification,
                product::SourceClassification::ExternalModule
            ),
        };
        state
            .namespaces
            .record_compilation_origin(source_key, CompilationOrigin::Library(ordinal));
        state.namespaces.source_units.push_local(SourceUnitRecord {
            source: source_key,
            origin: CompilationOrigin::Library(ordinal),
            module,
            context,
        });
        source_modules.push(module);
        source_keys.push(source_key);
        units.push(crate::binder::bind::LibraryBinderUnit {
            ordinal,
            source: source_key,
            module,
        });
    }
    state.namespaces.compilation_global = Some(compilation_global);
    state.namespaces.script_namespace_root = Some(script_namespace_root);
    state.namespaces.library_shared_globals = true;

    let mut container_scopes = vec![None; declaration_product.containers().len()];
    for (index, container) in declaration_product.containers().iter().enumerate() {
        let scope = match container.kind {
            product::ContainerKind::CompilationGlobal => compilation_global,
            product::ContainerKind::ExternalModule => {
                let anchor = container
                    .anchor
                    .ok_or(LibraryProductInstantiationError::MissingContainer(index))?;
                *source_modules.get(anchor.source.index()).ok_or(
                    LibraryProductInstantiationError::MissingSource(anchor.source.index()),
                )?
            }
            product::ContainerKind::Namespace => {
                container
                    .parent
                    .and_then(|key| container_scopes.get(key.index()).copied().flatten())
                    .ok_or(LibraryProductInstantiationError::MissingContainer(index))?;
                state
                    .graph
                    .push(Scope::new(ScopeKind::NamespacePublic, None))
            }
            product::ContainerKind::AmbientExternalModule => {
                return Err(
                    LibraryProductInstantiationError::UnsupportedAmbientExternalModule(index),
                );
            }
        };
        container_scopes[index] = Some(scope);
    }

    let mut syntax_region_scopes = vec![None; declaration_product.regions().len()];
    let mut lexical_region_scopes = vec![None; declaration_product.regions().len()];
    for (index, region) in declaration_product.regions().iter().enumerate() {
        let source_module = *source_modules.get(region.site.source.index()).ok_or(
            LibraryProductInstantiationError::MissingSource(region.site.source.index()),
        )?;
        let (syntax, lexical) = match region.kind {
            product::RegionKind::Source => {
                let source = declaration_product
                    .sources()
                    .get(region.site.source.index())
                    .ok_or(LibraryProductInstantiationError::MissingSource(
                        region.site.source.index(),
                    ))?;
                let lexical = match source.classification {
                    product::SourceClassification::Script => compilation_global,
                    product::SourceClassification::ExternalModule => source_module,
                };
                (source_module, lexical)
            }
            product::RegionKind::Namespace => {
                let parent = region
                    .parent
                    .and_then(|key| lexical_region_scopes.get(key.index()).copied().flatten())
                    .ok_or(LibraryProductInstantiationError::MissingRegion(index))?;
                let public = container_scopes
                    .get(region.container.index())
                    .copied()
                    .flatten()
                    .ok_or(LibraryProductInstantiationError::MissingContainer(
                        region.container.index(),
                    ))?;
                let private = state.graph.push(
                    Scope::new(ScopeKind::NamespacePrivate, Some(parent))
                        .with_namespace_public(public),
                );
                (private, private)
            }
            product::RegionKind::GlobalAugmentation => {
                let parent = region
                    .parent
                    .and_then(|key| lexical_region_scopes.get(key.index()).copied().flatten())
                    .ok_or(LibraryProductInstantiationError::MissingRegion(index))?;
                let overlay = state
                    .graph
                    .push(Scope::new(ScopeKind::GlobalOverlay, Some(parent)));
                (overlay, compilation_global)
            }
            product::RegionKind::AmbientExternalModule => {
                return Err(
                    LibraryProductInstantiationError::UnsupportedAmbientExternalModule(index),
                );
            }
        };
        syntax_region_scopes[index] = Some(syntax);
        lexical_region_scopes[index] = Some(lexical);
    }

    let mut callable_scopes = Vec::with_capacity(declaration_product.callable_scopes().len());
    let mut fn_scopes = FxHashMap::default();
    for (index, callable) in declaration_product.callable_scopes().iter().enumerate() {
        let parent =
            match callable.parent {
                product::LexicalScopeParent::Region(region) => lexical_region_scopes
                    .get(region.index())
                    .copied()
                    .flatten()
                    .ok_or(LibraryProductInstantiationError::MissingRegion(
                        region.index(),
                    ))?,
                product::LexicalScopeParent::Callable(parent) => *callable_scopes
                    .get(parent.index())
                    .ok_or(LibraryProductInstantiationError::MissingRegion(index))?,
            };
        let scope = state
            .graph
            .push(Scope::new(ScopeKind::Function, Some(parent)));
        let module = *source_modules.get(callable.site.source.index()).ok_or(
            LibraryProductInstantiationError::MissingSource(callable.site.source.index()),
        )?;
        fn_scopes.insert((module, callable.site.start), scope);
        callable_scopes.push(scope);
    }

    let mut declaration_ids = Vec::with_capacity(declaration_product.declarations().len());
    for (index, declaration) in declaration_product.declarations().iter().enumerate() {
        let module = *source_modules.get(declaration.site.source.index()).ok_or(
            LibraryProductInstantiationError::MissingSource(declaration.site.source.index()),
        )?;
        let scope = match declaration.lexical_scope {
            product::DeclarationLexicalScope::None => None,
            product::DeclarationLexicalScope::Region(region) => Some(
                lexical_region_scopes
                    .get(region.index())
                    .copied()
                    .flatten()
                    .ok_or(LibraryProductInstantiationError::MissingRegion(
                        region.index(),
                    ))?,
            ),
            product::DeclarationLexicalScope::Callable(callable) => Some(
                *callable_scopes
                    .get(callable.index())
                    .ok_or(LibraryProductInstantiationError::MissingRegion(index))?,
            ),
        };
        let id = state.declarations.push(
            declaration_kind(declaration.kind())?,
            DeclarationSite {
                module,
                scope,
                declaration_span: Span::new(declaration.site.start, declaration.site.end),
                binding_span: Span::new(
                    declaration.binding_site.start,
                    declaration.binding_site.end,
                ),
            },
        );
        declaration_ids.push(id);
    }

    let mut declaration_groups = vec![None; declaration_ids.len()];
    for (group_index, group) in declaration_product.groups().iter().enumerate() {
        for declaration in &group.fragments {
            let slot = declaration_groups.get_mut(declaration.index()).ok_or(
                LibraryProductInstantiationError::MissingDeclaration(declaration.index()),
            )?;
            *slot = Some(group_index);
        }
    }

    let mut group_type_ids = vec![None; declaration_product.groups().len()];
    for (group_index, group) in declaration_product.groups().iter().enumerate() {
        if !group.slots.r#type {
            continue;
        }
        let name = string(group.name)?;
        let mut current = None;
        for declaration_key in &group.fragments {
            let declaration = declaration_product
                .declarations()
                .get(declaration_key.index())
                .ok_or(LibraryProductInstantiationError::MissingDeclaration(
                    declaration_key.index(),
                ))?;
            let Some(kind) = type_fragment_kind(declaration.kind()) else {
                continue;
            };
            let id = *declaration_ids.get(declaration_key.index()).ok_or(
                LibraryProductInstantiationError::MissingDeclaration(declaration_key.index()),
            )?;
            let row = state.declarations.get(id).ok_or(
                LibraryProductInstantiationError::MissingDeclaration(id.index()),
            )?;
            let source = *source_keys.get(declaration.site.source.index()).ok_or(
                LibraryProductInstantiationError::MissingSource(declaration.site.source.index()),
            )?;
            let appended = state.type_groups.append_fragment(
                current,
                name,
                TypeGroupFragment {
                    declaration: id,
                    source,
                    scope: row.site.scope.ok_or(
                        LibraryProductInstantiationError::MissingDeclaration(id.index()),
                    )?,
                    site: row.site,
                    kind,
                },
            );
            current = Some(appended.group);
            state
                .declarations
                .get_mut(id)
                .ok_or(LibraryProductInstantiationError::MissingDeclaration(
                    id.index(),
                ))?
                .type_group = current;
        }
        group_type_ids[group_index] = current;
    }

    let mut declaration_values = vec![None; declaration_ids.len()];
    for (index, declaration) in declaration_product.declarations().iter().enumerate() {
        if !declaration.spaces.value || declaration.kind() == product::DeclarationKind::Namespace {
            continue;
        }
        let storage = state.fresh_value_storage();
        let id = declaration_ids[index];
        state
            .declarations
            .get_mut(id)
            .ok_or(LibraryProductInstantiationError::MissingDeclaration(index))?
            .value_storage = Some(storage);
        declaration_values[index] = Some(storage);
    }

    let mut group_symbols = Vec::with_capacity(declaration_product.groups().len());
    let mut group_locations = Vec::with_capacity(declaration_product.groups().len());
    let mut symbol_by_container_name = BTreeMap::new();
    for (group_index, group) in declaration_product.groups().iter().enumerate() {
        let name = string(group.name)?;
        let container_scope = container_scopes
            .get(group.container.index())
            .copied()
            .flatten()
            .ok_or(LibraryProductInstantiationError::MissingContainer(
                group.container.index(),
            ))?;
        let mut local_scope = None;
        let mut saw_container_scope = false;
        for declaration in &group.fragments {
            let recipe = declaration_product
                .declarations()
                .get(declaration.index())
                .ok_or(LibraryProductInstantiationError::MissingDeclaration(
                    declaration.index(),
                ))?;
            let candidate = match recipe.lexical_scope {
                product::DeclarationLexicalScope::Callable(callable) => {
                    Some(*callable_scopes.get(callable.index()).ok_or(
                        LibraryProductInstantiationError::MissingRegion(callable.index()),
                    )?)
                }
                product::DeclarationLexicalScope::Region(region) if !group.slots.r#type => {
                    let region_recipe = declaration_product.regions().get(region.index()).ok_or(
                        LibraryProductInstantiationError::MissingRegion(region.index()),
                    )?;
                    if region_recipe.kind == product::RegionKind::Namespace {
                        Some(lexical_region_scopes[region.index()].ok_or(
                            LibraryProductInstantiationError::MissingRegion(region.index()),
                        )?)
                    } else {
                        None
                    }
                }
                product::DeclarationLexicalScope::None
                | product::DeclarationLexicalScope::Region(_) => None,
            };
            if let Some(candidate) = candidate {
                if saw_container_scope {
                    return Err(LibraryProductInstantiationError::MixedGroupLexicalScope(
                        group_index,
                    ));
                }
                if local_scope
                    .replace(candidate)
                    .is_some_and(|scope| scope != candidate)
                {
                    return Err(LibraryProductInstantiationError::MixedGroupLexicalScope(
                        group_index,
                    ));
                }
            } else if local_scope.is_some() {
                return Err(LibraryProductInstantiationError::MixedGroupLexicalScope(
                    group_index,
                ));
            } else {
                saw_container_scope = true;
            }
        }
        let scope = local_scope.unwrap_or(container_scope);
        let mut symbol = Symbol::new(name);
        symbol.ty = group_type_ids[group_index];
        symbol.owns_type_group = symbol.ty.is_some();
        symbol.value = group
            .fragments
            .iter()
            .rev()
            .find_map(|key| declaration_values.get(key.index()).copied().flatten());
        symbol.function_values = group
            .fragments
            .iter()
            .filter(|key| {
                declaration_product.declarations()[key.index()].kind()
                    == product::DeclarationKind::Function
            })
            .filter_map(|key| declaration_values.get(key.index()).copied().flatten())
            .collect();
        symbol.declarations = group
            .fragments
            .iter()
            .filter_map(|key| declaration_ids.get(key.index()).copied())
            .collect();
        let key = (scope.0, name.to_owned());
        let symbol_id = if let Some(existing) = symbol_by_container_name.get(&key).copied() {
            let row = state
                .symbols
                .get_mut(existing)
                .ok_or(LibraryProductInstantiationError::MissingGroup(group_index))?;
            if row.ty.is_some() && symbol.ty.is_some()
                || row.value.is_some() && symbol.value.is_some()
            {
                return Err(
                    LibraryProductInstantiationError::DuplicateScopeDeclaration {
                        scope: scope.0,
                        name: name.to_owned(),
                    },
                );
            }
            row.ty = row.ty.or(symbol.ty);
            row.owns_type_group |= symbol.owns_type_group;
            row.value = row.value.or(symbol.value);
            row.function_values.extend(symbol.function_values);
            row.declarations.extend(symbol.declarations);
            existing
        } else {
            let symbol_id = state.symbols.push(symbol);
            if state
                .graph
                .declare(scope, name.to_owned(), symbol_id)
                .map_err(
                    |_| LibraryProductInstantiationError::DuplicateScopeDeclaration {
                        scope: scope.0,
                        name: name.to_owned(),
                    },
                )?
                .is_some()
            {
                return Err(
                    LibraryProductInstantiationError::DuplicateScopeDeclaration {
                        scope: scope.0,
                        name: name.to_owned(),
                    },
                );
            }
            symbol_by_container_name.insert(key, symbol_id);
            symbol_id
        };
        group_symbols.push(symbol_id);
        let first_key = group
            .fragments
            .first()
            .ok_or(LibraryProductInstantiationError::MissingGroup(group_index))?;
        let first = declaration_product
            .declarations()
            .get(first_key.index())
            .ok_or(LibraryProductInstantiationError::MissingGroup(group_index))?;
        let final_scope = match first.lexical_scope {
            product::DeclarationLexicalScope::Callable(callable) => {
                *callable_scopes.get(callable.index()).ok_or(
                    LibraryProductInstantiationError::MissingRegion(callable.index()),
                )?
            }
            product::DeclarationLexicalScope::Region(region)
                if declaration_product
                    .regions()
                    .get(region.index())
                    .is_some_and(|region| region.kind == product::RegionKind::Namespace) =>
            {
                container_scope
            }
            product::DeclarationLexicalScope::Region(region) if !group.slots.r#type => {
                lexical_region_scopes
                    .get(region.index())
                    .copied()
                    .flatten()
                    .ok_or(LibraryProductInstantiationError::MissingRegion(
                        region.index(),
                    ))?
            }
            product::DeclarationLexicalScope::None
            | product::DeclarationLexicalScope::Region(_) => container_scope,
        };
        group_locations.push(LibraryProductGroupLocation {
            scope: final_scope,
            name: name.into(),
        });
    }

    let mut namespace_group_by_declaration = BTreeMap::new();
    for row in declaration_product.namespace_attachment_rows() {
        namespace_group_by_declaration.insert(row.declaration.index(), row.group.index());
    }
    let mut namespace_ids_by_container = vec![None; declaration_product.containers().len()];
    let mut namespace_fragments_by_declaration = BTreeMap::new();
    let mut namespace_fragments_by_region = vec![None; declaration_product.regions().len()];
    for (declaration_index, declaration) in declaration_product.declarations().iter().enumerate() {
        let product::DeclarationPayload::Namespace { child_region } = declaration.payload else {
            continue;
        };
        let group_index = *namespace_group_by_declaration
            .get(&declaration_index)
            .ok_or(LibraryProductInstantiationError::MissingNamespaceGroup(
                declaration_index,
            ))?;
        let region = declaration_product
            .regions()
            .get(child_region.index())
            .ok_or(LibraryProductInstantiationError::MissingRegion(
                child_region.index(),
            ))?;
        let container_index = region.container.index();
        let namespace = match namespace_ids_by_container[container_index] {
            Some(id) => id,
            None => {
                let container = declaration_product
                    .containers()
                    .get(container_index)
                    .ok_or(LibraryProductInstantiationError::MissingContainer(
                        container_index,
                    ))?;
                let owner = namespace_owner_for_container(
                    container.parent,
                    declaration_product,
                    &namespace_ids_by_container,
                )?;
                let public_scope = container_scopes[container_index].ok_or(
                    LibraryProductInstantiationError::MissingNamespaceContainer(container_index),
                )?;
                let symbol = *group_symbols
                    .get(group_index)
                    .ok_or(LibraryProductInstantiationError::MissingGroup(group_index))?;
                let name = string(declaration.name)?.to_owned();
                let id = NamespaceId(u32::try_from(state.namespaces.namespaces.len()).map_err(
                    |_| {
                        LibraryProductInstantiationError::MissingNamespaceContainer(container_index)
                    },
                )?);
                state.namespaces.namespaces.push_local(Namespace {
                    id,
                    owner,
                    name: name.clone(),
                    public_scope,
                    symbol,
                    fragments: Vec::new(),
                });
                state
                    .namespaces
                    .aggregate_instance_states
                    .push_local(NamespaceInstanceState::NonInstantiated);
                state.namespaces.standalone_value_storages.push_local(None);
                state
                    .namespaces
                    .namespace_keys
                    .insert_local(NamespaceKey { owner, name }, id)
                    .map_err(|_| {
                        LibraryProductInstantiationError::MissingNamespaceContainer(container_index)
                    })?;
                state
                    .symbols
                    .get_mut(symbol)
                    .ok_or(LibraryProductInstantiationError::MissingGroup(group_index))?
                    .ns = Some(id);
                namespace_ids_by_container[container_index] = Some(id);
                id
            }
        };
        let fragment_id = NamespaceFragmentId(
            u32::try_from(state.namespaces.fragments.len()).map_err(|_| {
                LibraryProductInstantiationError::MissingDeclaration(declaration_index)
            })?,
        );
        let module = source_modules[declaration.site.source.index()];
        let source = source_keys[declaration.site.source.index()];
        let ordinal = library_ordinal(declaration_product, declaration.site.source.index())?;
        let private_scope = lexical_region_scopes[child_region.index()].ok_or(
            LibraryProductInstantiationError::MissingRegion(child_region.index()),
        )?;
        let lexical_parent = region
            .parent
            .and_then(|parent| lexical_region_scopes[parent.index()])
            .ok_or(LibraryProductInstantiationError::MissingRegion(
                child_region.index(),
            ))?;
        let instance_state = namespace_instance_state(declaration_product, child_region);
        state.namespaces.fragments.push_local(NamespaceFragment {
            id: fragment_id,
            namespace,
            declaration: declaration_ids[declaration_index],
            source,
            origin: CompilationOrigin::Library(ordinal),
            source_start: declaration.site.start,
            module,
            private_scope,
            lexical_parent,
            public_scope: container_scopes[container_index].ok_or(
                LibraryProductInstantiationError::MissingNamespaceContainer(container_index),
            )?,
            ambient: declaration.flags.ambient,
            publication: NamespacePublication::AmbientDefault,
            instance_state,
            members: Vec::new(),
        });
        state
            .namespaces
            .namespaces
            .get_mut_local(namespace.index())
            .ok_or(LibraryProductInstantiationError::MissingNamespaceContainer(
                container_index,
            ))?
            .fragments
            .push(fragment_id);
        state
            .declarations
            .get_mut(declaration_ids[declaration_index])
            .ok_or(LibraryProductInstantiationError::MissingDeclaration(
                declaration_index,
            ))?
            .namespace = Some(namespace);
        namespace_fragments_by_declaration.insert(declaration_index, fragment_id);
        namespace_fragments_by_region[child_region.index()] = Some(fragment_id);
    }

    let mut global_by_region = BTreeMap::new();
    for (declaration_index, declaration) in declaration_product.declarations().iter().enumerate() {
        let product::DeclarationPayload::Global { child_region } = declaration.payload else {
            continue;
        };
        let region = declaration_product
            .regions()
            .get(child_region.index())
            .ok_or(LibraryProductInstantiationError::MissingRegion(
                child_region.index(),
            ))?;
        let id =
            GlobalAugmentationId(u32::try_from(state.namespaces.globals.len()).map_err(|_| {
                LibraryProductInstantiationError::MissingDeclaration(declaration_index)
            })?);
        let module = source_modules[declaration.site.source.index()];
        let source = source_keys[declaration.site.source.index()];
        let origin = CompilationOrigin::Library(library_ordinal(
            declaration_product,
            declaration.site.source.index(),
        )?);
        let overlay_scope = syntax_region_scopes[child_region.index()].ok_or(
            LibraryProductInstantiationError::MissingRegion(child_region.index()),
        )?;
        let parent_scope = region
            .parent
            .and_then(|parent| lexical_region_scopes[parent.index()])
            .ok_or(LibraryProductInstantiationError::MissingRegion(
                child_region.index(),
            ))?;
        state.namespaces.globals.push_local(GlobalAugmentation {
            id,
            declaration: declaration_ids[declaration_index],
            source,
            origin,
            module,
            owner: GlobalOwner::Lexical(parent_scope),
            body_span: Span::new(region.site.start, region.site.end),
            diagnostic_span: Span::new(declaration.site.start, declaration.site.end),
            target_scope: compilation_global,
            overlay_scope,
            placement: if matches!(
                declaration_product.sources()[declaration.site.source.index()].classification,
                product::SourceClassification::ExternalModule
            ) {
                GlobalPlacement::DirectExternalModule
            } else {
                GlobalPlacement::DirectScript
            },
            issues: Vec::new(),
            declared: declaration.flags.ambient,
            members: Vec::new(),
        });
        global_by_region.insert(child_region.index(), id);
    }

    for (group_index, group) in declaration_product.groups().iter().enumerate() {
        let owner = declaration_owner_for_container(
            group.container,
            declaration_product,
            &namespace_ids_by_container,
            &source_modules,
            compilation_global,
        )?;
        let name = string(group.name)?;
        for declaration_key in &group.fragments {
            let declaration_index = declaration_key.index();
            let declaration = &declaration_product.declarations()[declaration_index];
            if matches!(
                declaration.kind(),
                product::DeclarationKind::Parameter | product::DeclarationKind::CatchParameter
            ) {
                continue;
            }
            let source_index = declaration.site.source.index();
            let unit = CompilationUnit {
                source: source_keys[source_index],
                origin: CompilationOrigin::Library(library_ordinal(
                    declaration_product,
                    source_index,
                )?),
                binding: ModuleBindingContext {
                    source_file_kind: SourceFileKind::DeclarationTs,
                    external_module: matches!(
                        declaration_product.sources()[source_index].classification,
                        product::SourceClassification::ExternalModule
                    ),
                },
            };
            let namespace_instance = namespace_fragments_by_declaration
                .get(&declaration_index)
                .and_then(|fragment| state.namespaces.fragment(*fragment))
                .map(|row| row.instance_state);
            let participant = push_placement(
                state,
                owner,
                name,
                declaration_ids[declaration_index],
                merge_kind(declaration.kind())?,
                spaces(declaration.spaces),
                declaration.flags.ambient,
                unit,
                syntax(declaration),
            )
            .ok_or(LibraryProductInstantiationError::MissingGroup(group_index))?;
            if let Some(fragment) = namespace_fragments_by_declaration.get(&declaration_index) {
                participant.namespace_fragment = Some(*fragment);
                participant.namespace_instance = namespace_instance;
            }
        }
    }

    let regions_by_declaration = declaration_product
        .declarations()
        .iter()
        .enumerate()
        .map(|(index, declaration)| (index, declaration.region.index()))
        .collect::<Vec<_>>();
    for (declaration_index, region_index) in regions_by_declaration {
        #[cfg(test)]
        record_namespace_region_index_probe();
        let Some(fragment) = namespace_fragments_by_region
            .get(region_index)
            .copied()
            .flatten()
        else {
            continue;
        };
        if namespace_fragments_by_declaration.contains_key(&declaration_index) {
            continue;
        }
        let declaration = &declaration_product.declarations()[declaration_index];
        if matches!(
            declaration.kind(),
            product::DeclarationKind::Parameter | product::DeclarationKind::CatchParameter
        ) {
            continue;
        }
        let namespace = state
            .namespaces
            .fragment(fragment)
            .ok_or(LibraryProductInstantiationError::MissingDeclaration(
                declaration_index,
            ))?
            .namespace;
        let unit = library_unit(
            declaration_product,
            declaration.site.source.index(),
            &source_keys,
        )?;
        push_member(
            state,
            WalkContext {
                owner: DeclarationOwner::NamespacePublic(namespace),
                lexical_scope: lexical_region_scopes[region_index].ok_or(
                    LibraryProductInstantiationError::MissingRegion(region_index),
                )?,
                namespace: Some((namespace, fragment)),
                global: None,
                deferred_module: None,
                ambient: true,
                ambient_export_list_mode: false,
                active_export_context: None,
                direct_top_level: false,
            },
            DeclarationOwner::NamespacePublic(namespace),
            Some(declaration_ids[declaration_index]),
            Some(string(declaration.name)?.to_owned()),
            Span::new(declaration.site.start, declaration.site.end),
            Span::new(declaration.binding_site.start, declaration.binding_site.end),
            spaces(declaration.spaces),
            merge_kind(declaration.kind())?,
            NamespacePublication::AmbientDefault,
            unit,
        );
        let group_index = declaration_groups[declaration_index].ok_or(
            LibraryProductInstantiationError::MissingGroup(declaration_index),
        )?;
        let source_symbol = *group_symbols
            .get(group_index)
            .ok_or(LibraryProductInstantiationError::MissingGroup(group_index))?;
        let public_scope = state
            .namespaces
            .get(namespace)
            .ok_or(LibraryProductInstantiationError::MissingNamespaceContainer(
                namespace.index(),
            ))?
            .public_scope;
        let target_symbol = state
            .graph
            .get(public_scope)
            .and_then(|scope| scope.lookup_local(string(declaration.name).ok()?))
            .ok_or(LibraryProductInstantiationError::MissingGroup(group_index))?;
        merge_symbol_slots(state, source_symbol, target_symbol, group_index)?;
        group_symbols[group_index] = target_symbol;
    }
    for (declaration_index, declaration) in declaration_product.declarations().iter().enumerate() {
        if matches!(
            declaration.kind(),
            product::DeclarationKind::Parameter | product::DeclarationKind::CatchParameter
        ) {
            continue;
        }
        let Some(global) = global_by_region.get(&declaration.region.index()).copied() else {
            continue;
        };
        if matches!(
            declaration.payload,
            product::DeclarationPayload::Global { .. }
        ) {
            continue;
        }
        let unit = library_unit(
            declaration_product,
            declaration.site.source.index(),
            &source_keys,
        )?;
        push_member(
            state,
            WalkContext {
                owner: DeclarationOwner::CompilationGlobal,
                lexical_scope: lexical_region_scopes[declaration.region.index()].ok_or(
                    LibraryProductInstantiationError::MissingRegion(declaration.region.index()),
                )?,
                namespace: None,
                global: Some(global),
                deferred_module: None,
                ambient: true,
                ambient_export_list_mode: false,
                active_export_context: None,
                direct_top_level: false,
            },
            DeclarationOwner::CompilationGlobal,
            Some(declaration_ids[declaration_index]),
            Some(string(declaration.name)?.to_owned()),
            Span::new(declaration.site.start, declaration.site.end),
            Span::new(declaration.binding_site.start, declaration.binding_site.end),
            spaces(declaration.spaces),
            merge_kind(declaration.kind())?,
            NamespacePublication::AmbientDefault,
            unit,
        );
    }

    state
        .namespaces
        .classify()
        .map_err(LibraryProductInstantiationError::NamespaceClassification)?;

    let mut fn_decl_ids = FxHashMap::default();
    for (index, declaration) in declaration_product.declarations().iter().enumerate() {
        if declaration.kind() != product::DeclarationKind::Function {
            continue;
        }
        let Some(storage) = declaration_values[index] else {
            continue;
        };
        let module = source_modules[declaration.site.source.index()];
        let start = declaration_product
            .callable_scopes()
            .iter()
            .find(|scope| {
                scope.site.source == declaration.site.source
                    && scope.site.start == declaration.site.start
                    && scope.site.end == declaration.site.end
            })
            .map_or(declaration.site.start, |scope| scope.site.start);
        fn_decl_ids.insert((module, start), storage);
    }

    let final_module = source_modules
        .last()
        .copied()
        .ok_or(LibraryProductInstantiationError::MissingSource(0))?;
    Ok(LibraryProductInstantiationFacts {
        units,
        final_module,
        fn_scopes,
        fn_decl_ids,
        map: LibraryProductBinderMap {
            sources: source_keys
                .into_iter()
                .zip(source_modules)
                .map(|(source, module)| LibraryProductSourceBinding { source, module })
                .collect(),
            containers: container_scopes
                .into_iter()
                .collect::<Option<Vec<_>>>()
                .ok_or(LibraryProductInstantiationError::MissingContainer(0))?
                .into_boxed_slice(),
            regions: syntax_region_scopes
                .into_iter()
                .zip(lexical_region_scopes)
                .map(|(syntax_scope, lexical_scope)| {
                    Some(LibraryProductRegionBinding {
                        syntax_scope: syntax_scope?,
                        lexical_scope: lexical_scope?,
                    })
                })
                .collect::<Option<Vec<_>>>()
                .ok_or(LibraryProductInstantiationError::MissingRegion(0))?
                .into_boxed_slice(),
            callable_scopes: callable_scopes.into_boxed_slice(),
            declarations: declaration_ids
                .into_iter()
                .enumerate()
                .map(|(index, declaration)| LibraryProductDeclarationBinding {
                    declaration,
                    value: declaration_values[index],
                    namespace_fragment: namespace_fragments_by_declaration.get(&index).copied(),
                })
                .collect(),
            groups: group_symbols
                .into_iter()
                .enumerate()
                .map(|(index, symbol)| LibraryProductGroupBinding {
                    symbol,
                    ty: group_type_ids[index],
                    value: None,
                    namespace: None,
                })
                .collect(),
            group_locations: group_locations.into_boxed_slice(),
        },
    })
}

fn declaration_kind(
    kind: product::DeclarationKind,
) -> Result<DeclarationKind, LibraryProductInstantiationError> {
    Ok(match kind {
        product::DeclarationKind::Variable => DeclarationKind::Variable,
        product::DeclarationKind::Function => DeclarationKind::Function,
        product::DeclarationKind::Class => DeclarationKind::Class,
        product::DeclarationKind::Parameter => DeclarationKind::Parameter,
        product::DeclarationKind::CatchParameter => DeclarationKind::CatchParameter,
        product::DeclarationKind::Import => DeclarationKind::Import,
        product::DeclarationKind::TypeAlias => DeclarationKind::TypeAlias,
        product::DeclarationKind::Interface => DeclarationKind::Interface,
        product::DeclarationKind::Enum => DeclarationKind::Enum,
        product::DeclarationKind::Namespace => DeclarationKind::Namespace,
        product::DeclarationKind::ImportEquals => DeclarationKind::ImportEquals,
        product::DeclarationKind::NamespaceExport => DeclarationKind::NamespaceExport,
        product::DeclarationKind::Global => DeclarationKind::Global,
        product::DeclarationKind::DefaultExportExpression => {
            DeclarationKind::DefaultExportExpression
        }
    })
}

fn type_fragment_kind(kind: product::DeclarationKind) -> Option<TypeFragmentKind> {
    match kind {
        product::DeclarationKind::TypeAlias => Some(TypeFragmentKind::TypeAlias),
        product::DeclarationKind::Interface => Some(TypeFragmentKind::Interface),
        product::DeclarationKind::Class => Some(TypeFragmentKind::Class),
        product::DeclarationKind::Enum => Some(TypeFragmentKind::Enum),
        _ => None,
    }
}

fn merge_kind(
    kind: product::DeclarationKind,
) -> Result<MergeDeclarationKind, LibraryProductInstantiationError> {
    Ok(match kind {
        product::DeclarationKind::Variable => MergeDeclarationKind::Variable,
        product::DeclarationKind::Function => MergeDeclarationKind::Function,
        product::DeclarationKind::Class => MergeDeclarationKind::Class,
        product::DeclarationKind::TypeAlias => MergeDeclarationKind::TypeAlias,
        product::DeclarationKind::Interface => MergeDeclarationKind::Interface,
        product::DeclarationKind::Enum => MergeDeclarationKind::Enum,
        product::DeclarationKind::Namespace => MergeDeclarationKind::Namespace,
        product::DeclarationKind::Import | product::DeclarationKind::ImportEquals => {
            MergeDeclarationKind::ImportAlias
        }
        product::DeclarationKind::NamespaceExport
        | product::DeclarationKind::DefaultExportExpression => MergeDeclarationKind::DeferredExport,
        product::DeclarationKind::Parameter
        | product::DeclarationKind::CatchParameter
        | product::DeclarationKind::Global => {
            return Err(LibraryProductInstantiationError::UnsupportedDeclarationKind(kind));
        }
    })
}

fn spaces(slots: product::DeclarationSlots) -> DeclarationSpaces {
    DeclarationSpaces {
        value: slots.value,
        r#type: slots.r#type,
        namespace: slots.namespace,
    }
}

fn merge_symbol_slots(
    state: &mut BindState,
    source: crate::binder::symbol::SymbolId,
    target: crate::binder::symbol::SymbolId,
    group: usize,
) -> Result<(), LibraryProductInstantiationError> {
    if source == target {
        return Ok(());
    }
    let source = state
        .symbols
        .get(source)
        .cloned()
        .ok_or(LibraryProductInstantiationError::MissingGroup(group))?;
    let target = state
        .symbols
        .get_mut(target)
        .ok_or(LibraryProductInstantiationError::MissingGroup(group))?;
    target.value = target.value.or(source.value);
    target.function_values.extend(source.function_values);
    target.ty = target.ty.or(source.ty);
    target.owns_type_group |= source.owns_type_group;
    target.ns = target.ns.or(source.ns);
    Ok(())
}

fn syntax(declaration: &product::DeclarationRecipe) -> DeclarationSyntaxFacts {
    match declaration.kind() {
        product::DeclarationKind::Variable => {
            DeclarationSyntaxFacts::Variable(if declaration.flags.constant {
                VariableKind::Const
            } else {
                VariableKind::Var
            })
        }
        product::DeclarationKind::Enum => DeclarationSyntaxFacts::Enum {
            constant: declaration.flags.constant,
        },
        _ => DeclarationSyntaxFacts::None,
    }
}

fn namespace_owner_for_container(
    parent: Option<product::ContainerKey>,
    declaration_product: &product::LibraryDeclarationProduct,
    namespaces: &[Option<NamespaceId>],
) -> Result<NamespaceOwner, LibraryProductInstantiationError> {
    let Some(parent) = parent else {
        return Ok(NamespaceOwner::CompilationGlobal);
    };
    match declaration_product
        .containers()
        .get(parent.index())
        .ok_or(LibraryProductInstantiationError::MissingContainer(
            parent.index(),
        ))?
        .kind
    {
        product::ContainerKind::CompilationGlobal => Ok(NamespaceOwner::CompilationGlobal),
        product::ContainerKind::Namespace => namespaces
            .get(parent.index())
            .copied()
            .flatten()
            .map(NamespaceOwner::NamespacePublic)
            .ok_or(LibraryProductInstantiationError::MissingNamespaceContainer(
                parent.index(),
            )),
        product::ContainerKind::ExternalModule | product::ContainerKind::AmbientExternalModule => {
            Err(LibraryProductInstantiationError::MissingNamespaceContainer(
                parent.index(),
            ))
        }
    }
}

fn declaration_owner_for_container(
    container: product::ContainerKey,
    declaration_product: &product::LibraryDeclarationProduct,
    namespaces: &[Option<NamespaceId>],
    source_modules: &[ScopeId],
    compilation_global: ScopeId,
) -> Result<DeclarationOwner, LibraryProductInstantiationError> {
    let row = declaration_product
        .containers()
        .get(container.index())
        .ok_or(LibraryProductInstantiationError::MissingContainer(
            container.index(),
        ))?;
    match row.kind {
        product::ContainerKind::CompilationGlobal => Ok(DeclarationOwner::CompilationGlobal),
        product::ContainerKind::Namespace => namespaces
            .get(container.index())
            .copied()
            .flatten()
            .map(DeclarationOwner::NamespacePublic)
            .ok_or(LibraryProductInstantiationError::MissingNamespaceContainer(
                container.index(),
            )),
        product::ContainerKind::ExternalModule => {
            let source = row
                .anchor
                .ok_or(LibraryProductInstantiationError::MissingContainer(
                    container.index(),
                ))?
                .source;
            source_modules
                .get(source.index())
                .copied()
                .map(DeclarationOwner::Lexical)
                .ok_or(LibraryProductInstantiationError::MissingSource(
                    source.index(),
                ))
        }
        product::ContainerKind::AmbientExternalModule => Err(
            LibraryProductInstantiationError::UnsupportedAmbientExternalModule(container.index()),
        ),
    }
    .map(|owner| match owner {
        DeclarationOwner::Lexical(scope) if scope == compilation_global => {
            DeclarationOwner::CompilationGlobal
        }
        owner => owner,
    })
}

fn namespace_instance_state(
    declaration_product: &product::LibraryDeclarationProduct,
    region: product::RegionKey,
) -> NamespaceInstanceState {
    if declaration_product
        .declarations()
        .iter()
        .any(|declaration| declaration.region == region && declaration.spaces.value)
    {
        NamespaceInstanceState::Instantiated
    } else {
        NamespaceInstanceState::NonInstantiated
    }
}

fn library_unit(
    declaration_product: &product::LibraryDeclarationProduct,
    source_index: usize,
    sources: &[SourceUnitKey],
) -> Result<CompilationUnit, LibraryProductInstantiationError> {
    Ok(CompilationUnit {
        source: sources[source_index],
        origin: CompilationOrigin::Library(library_ordinal(declaration_product, source_index)?),
        binding: ModuleBindingContext {
            source_file_kind: SourceFileKind::DeclarationTs,
            external_module: matches!(
                declaration_product.sources()[source_index].classification,
                product::SourceClassification::ExternalModule
            ),
        },
    })
}

fn library_ordinal(
    declaration_product: &product::LibraryDeclarationProduct,
    source_index: usize,
) -> Result<LibraryFileOrdinal, LibraryProductInstantiationError> {
    let source = declaration_product.sources().get(source_index).ok_or(
        LibraryProductInstantiationError::MissingSource(source_index),
    )?;
    usize::try_from(source.ordinal)
        .map(LibraryFileOrdinal::new)
        .map_err(|_| LibraryProductInstantiationError::MissingSource(source_index))
}

trait DeclarationRecipeKind {
    fn kind(&self) -> product::DeclarationKind;
}

impl DeclarationRecipeKind for product::DeclarationRecipe {
    fn kind(&self) -> product::DeclarationKind {
        match self.payload {
            product::DeclarationPayload::Variable { .. } => product::DeclarationKind::Variable,
            product::DeclarationPayload::Function { .. } => product::DeclarationKind::Function,
            product::DeclarationPayload::Class { .. } => product::DeclarationKind::Class,
            product::DeclarationPayload::Interface { .. } => product::DeclarationKind::Interface,
            product::DeclarationPayload::TypeAlias { .. } => product::DeclarationKind::TypeAlias,
            product::DeclarationPayload::Enum { .. } => product::DeclarationKind::Enum,
            product::DeclarationPayload::Namespace { .. } => product::DeclarationKind::Namespace,
            product::DeclarationPayload::Import => product::DeclarationKind::Import,
            product::DeclarationPayload::ImportEquals => product::DeclarationKind::ImportEquals,
            product::DeclarationPayload::NamespaceExport => {
                product::DeclarationKind::NamespaceExport
            }
            product::DeclarationPayload::Global { .. } => product::DeclarationKind::Global,
            product::DeclarationPayload::Parameter { .. } => product::DeclarationKind::Parameter,
            product::DeclarationPayload::CatchParameter { .. } => {
                product::DeclarationKind::CatchParameter
            }
            product::DeclarationPayload::DefaultExportExpression => {
                product::DeclarationKind::DefaultExportExpression
            }
        }
    }
}

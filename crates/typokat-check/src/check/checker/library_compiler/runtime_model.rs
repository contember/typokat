use super::*;
use typokat_types::alias_semantics::{PublishedAliasTerminal, PublishedAliases};

pub struct OwnedLibraryRuntimeState {
    pub(super) interner: Interner,
    pub(super) binder: Binder,
    pub(super) published_types: PublishedTypeEnvironment,
    pub(super) decl_types: DeclTypes,
    pub(super) semantic_identities: Option<LibrarySemanticIdentities>,
    pub(super) runtime: FrozenCheckerRuntimeMetadata,
    pub(super) next_type_param: u32,
    pub(super) next_class_id: u32,
    pub(super) source_file_count: u32,
    pub(super) library_modules: std::sync::Arc<[ScopeId]>,
    pub(super) replay_index: Option<Box<AdmittedCollisionReplayIndex>>,
    pub(super) private_collision_epoch: Option<PrivateCollisionEpoch>,
}

pub(in crate::check::checker) struct OwnedLibraryRuntimeProductParts {
    pub(in crate::check::checker) interner: Interner,
    pub(in crate::check::checker) binder: Binder,
    pub(in crate::check::checker) published_types:
        super::type_groups::PublishedTypeEnvironmentProductParts,
    pub(in crate::check::checker) decl_types: Vec<Option<TypeId>>,
    pub(in crate::check::checker) semantic_identities:
        Option<super::library_identities::LibrarySemanticIdentitiesProductParts>,
    pub(in crate::check::checker) runtime: super::FrozenCheckerRuntimeSnapshotParts,
    pub(in crate::check::checker) next_type_param: u32,
    pub(in crate::check::checker) next_class_id: u32,
    pub(in crate::check::checker) source_file_count: u32,
}

pub struct CompiledLibraryRuntimeProduct {
    pub(in crate::check::checker) _parts: OwnedLibraryRuntimeProductParts,
    pub _replay_index: AdmittedCollisionReplayIndex,
}

pub fn freeze_library_runtime_product(
    mut state: OwnedLibraryRuntimeState,
) -> Result<CompiledLibraryRuntimeProduct, &'static str> {
    let replay_index = state
        .replay_index
        .take()
        .ok_or("source library compiler did not produce a replay index")?;
    state
        .into_product_parts()
        .map(|parts| CompiledLibraryRuntimeProduct {
            _parts: parts,
            _replay_index: *replay_index,
        })
}

fn validate_library_source_prefix(
    binder: &Binder,
    source_file_count: u32,
) -> Result<(), &'static str> {
    if source_file_count == 0 {
        return Err("product library state has no source files");
    }
    let mut source_keys = binder
        .module_sources()
        .values()
        .map(|source| source.0)
        .collect::<Vec<_>>();
    if source_keys.is_empty() {
        return Err("product binder has no retained source ownership");
    }
    source_keys.sort_unstable();
    if source_keys.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("product binder repeats a retained source key");
    }
    let expected_len = source_file_count
        .checked_add(1)
        .ok_or("product source count overflows the prelude prefix")?;
    if u32::try_from(source_keys.len()).map_err(|_| "product source-key count does not fit u32")?
        != expected_len
        || source_keys
            .iter()
            .enumerate()
            .any(|(index, source)| *source != u32::try_from(index).unwrap_or(u32::MAX))
    {
        return Err("product binder source keys are not the contiguous prelude/library prefix");
    }
    if binder
        .module_sources()
        .keys()
        .any(|scope| binder.graph.get(*scope).is_none())
    {
        return Err("product binder source ownership refers to an unknown scope");
    }
    Ok(())
}

pub(super) fn validate_owned_library_product_parts(
    parts: &OwnedLibraryRuntimeProductParts,
) -> Result<(), &'static str> {
    fn type_in_range(id: TypeId, store_len: usize) -> bool {
        usize::try_from(id.0).is_ok_and(|index| index < store_len)
    }

    fn type_param_precedes_counter(id: TypeParamId, next_type_param: u32) -> bool {
        id.0 < next_type_param
    }

    validate_library_source_prefix(&parts.binder, parts.source_file_count)?;
    let authenticated_symbol = collect_root_rows(&parts.binder)
        .map_err(|_| "product binder root projection is invalid")?
        .into_iter()
        .find(|row| row.name == "Symbol")
        .and_then(|row| row.value);
    if parts
        .runtime
        .certified_library_values
        .symbol
        .is_some_and(|storage| storage.0 >= parts.binder.decl_count)
    {
        return Err("product certified Symbol value is out of range");
    }
    if parts.runtime.certified_library_values.symbol != authenticated_symbol {
        return Err("product certified Symbol value does not match the authenticated root");
    }
    if parts.published_types.groups.len() != parts.binder.type_groups.len() {
        return Err("product published type-group count does not match the binder");
    }
    if parts.decl_types.len()
        != usize::try_from(parts.binder.decl_count)
            .map_err(|_| "product binder storage count does not fit usize")?
    {
        return Err("product declaration types do not cover the binder storage prefix");
    }

    let store = parts.interner.store();
    let store_len = store.len();
    if parts
        .decl_types
        .iter()
        .flatten()
        .any(|ty| !type_in_range(*ty, store_len))
    {
        return Err("product declaration type is out of range");
    }

    let mut classes = BTreeSet::new();
    for (class, terminal) in &parts.published_types.classes {
        if !classes.insert(*class) {
            return Err("product class publication repeats a class id");
        }
        if class.0 >= parts.next_class_id {
            return Err("product class id collides with the next class counter");
        }
        let OwnedPublishedClassTerminal::Ready(surface) = terminal else {
            continue;
        };
        if surface
            .type_params()
            .iter()
            .any(|parameter| !type_param_precedes_counter(*parameter, parts.next_type_param))
        {
            return Err("product type parameter id collides with the next parameter counter");
        }
        if surface.class() != *class
            || !type_in_range(surface.instance_template(), store_len)
            || !type_in_range(surface.static_template(), store_len)
            || surface
                .constructor_template()
                .is_some_and(|ty| !type_in_range(ty, store_len))
        {
            return Err("product published class surface has an invalid reference");
        }
    }

    let mut aliases = BTreeMap::new();
    for (alias, terminal) in &parts.published_types.aliases {
        if aliases.insert(*alias, terminal).is_some() {
            return Err("product alias publication repeats an alias id");
        }
        let PublishedAliasTerminal::Ready(surface) = terminal else {
            continue;
        };
        PublishedAliases::validate_ready_surface(store, *alias, surface)
            .map_err(|_| "product published alias surface has an invalid reference")?;
        if surface
            .type_params()
            .iter()
            .any(|parameter| !type_param_precedes_counter(*parameter, parts.next_type_param))
        {
            return Err("product alias parameter collides with the next parameter counter");
        }
    }

    let mut class_group_names = BTreeMap::new();
    let mut alias_group_owners = BTreeSet::new();
    for (index, terminal) in parts.published_types.groups.iter().enumerate() {
        let PublishedTypeGroupTerminal::Ready(group) = terminal else {
            continue;
        };
        let group_id = TypeGroupId(
            u32::try_from(index).map_err(|_| "product type-group index does not fit u32")?,
        );
        if parts
            .binder
            .type_groups
            .get(group_id)
            .is_none_or(|binding| binding.name != group.name)
        {
            return Err("product published type-group name does not match the binder");
        }
        if group
            .parameters
            .iter()
            .any(|parameter| !type_param_precedes_counter(*parameter, parts.next_type_param))
            || group.parameter_defaults.iter().any(|default| {
                matches!(default, PublishedTypeParameterDefault::Ready(ty) if !type_in_range(*ty, store_len))
            })
            || group
                .conflict_alternatives
                .iter()
                .flat_map(|alternative| &alternative.types)
                .any(|ty| !type_in_range(*ty, store_len))
        {
            return Err("product published type group has an invalid type reference");
        }
        match group.surface {
            PublishedTypeGroupSurface::Template(ty) if !type_in_range(ty, store_len) => {
                return Err("product published type-group template is out of range")
            }
            PublishedTypeGroupSurface::Enum(ty) if !type_in_range(ty, store_len) => {
                return Err("product published enum type is out of range")
            }
            PublishedTypeGroupSurface::Class(class) => {
                if !classes.contains(&class) {
                    return Err("product published type group refers to an unknown class");
                }
                if class_group_names
                    .insert(class, group.name.as_str())
                    .is_some()
                {
                    return Err("product class identity is published by multiple type groups");
                }
            }
            PublishedTypeGroupSurface::Template(template) => {
                if let Some(alias) = store.alias_template_id(template) {
                    let Some(PublishedAliasTerminal::Ready(surface)) = aliases.get(&alias).copied()
                    else {
                        return Err("product alias type group has no ready alias publication");
                    };
                    if surface.template() != template || surface.type_params() != group.parameters {
                        return Err("product alias publication does not match its type group");
                    }
                    if !alias_group_owners.insert(alias) {
                        return Err("product alias identity is published by multiple type groups");
                    }
                }
            }
            PublishedTypeGroupSurface::Enum(_) => {}
        }
    }
    if class_group_names.keys().copied().collect::<BTreeSet<_>>() != classes {
        return Err("product published classes do not have exact type-group ownership");
    }
    let ready_aliases = aliases
        .iter()
        .filter_map(|(alias, terminal)| {
            matches!(terminal, PublishedAliasTerminal::Ready(_)).then_some(*alias)
        })
        .collect::<BTreeSet<_>>();
    if alias_group_owners != ready_aliases {
        return Err("product published aliases do not have exact type-group ownership");
    }

    for index in 0..store_len {
        let ty = TypeId(
            u32::try_from(index).map_err(|_| "product type-store length does not fit TypeId")?,
        );
        if store
            .type_param(ty)
            .is_some_and(|parameter| parameter.id.0 >= parts.next_type_param)
            || store.function_type(ty).is_some_and(|function| {
                function
                    .type_params
                    .iter()
                    .any(|parameter| parameter.id.0 >= parts.next_type_param)
            })
            || store.instantiation_type(ty).is_some_and(|instantiation| {
                instantiation
                    .args
                    .iter()
                    .any(|(parameter, _)| parameter.0 >= parts.next_type_param)
            })
        {
            return Err("product type parameter id collides with the next parameter counter");
        }
        if store.class_instance_type(ty).is_some_and(|instance| {
            instance.class.0 >= parts.next_class_id || !classes.contains(&instance.class)
        }) {
            return Err("product class instance refers to an unknown class identity");
        }
        if store.object_type(ty).is_some_and(|object| {
            object.properties.iter().any(|property| {
                property.declaring_class.is_some_and(|class| {
                    class.0 >= parts.next_class_id || !classes.contains(&class)
                })
            })
        }) {
            return Err("product object property refers to an unknown class identity");
        }
    }

    if let Some(identities) = &parts.semantic_identities {
        let expected_names = [
            "Array",
            "ReadonlyArray",
            "String",
            "Number",
            "Boolean",
            "RegExp",
            "Object",
            "CallableFunction",
        ];
        for (terminal, expected_name) in identities.iter().zip(expected_names) {
            let LibraryIdentityTerminal::Ready(identity) = terminal else {
                return Err("product installed semantic identities are not all ready");
            };
            if !type_in_range(identity.template, store_len)
                || identity
                    .parameters
                    .iter()
                    .any(|parameter| parameter.0 >= parts.next_type_param)
            {
                return Err("product semantic identity has an invalid type reference");
            }
            let Some(PublishedTypeGroupTerminal::Ready(group)) =
                parts.published_types.groups.get(identity.group.index())
            else {
                return Err("product semantic identity refers to an unpublished type group");
            };
            if group.name != expected_name
                || group.parameters != identity.parameters
                || group.surface != PublishedTypeGroupSurface::Template(identity.template)
            {
                return Err("product semantic identity does not match its published type group");
            }
        }
    }

    let valid_class = |class: ClassId| class.0 < parts.next_class_id && classes.contains(&class);
    let valid_storage = |storage: ValueStorageId| storage.0 < parts.binder.decl_count;
    let application_classes = parts
        .runtime
        .class_application_parameters
        .iter()
        .map(|(class, _)| *class)
        .collect::<BTreeSet<_>>();
    if application_classes != classes {
        return Err("product class application metadata does not exactly cover published classes");
    }
    let new_metadata_classes = parts
        .runtime
        .class_new_metadata
        .iter()
        .map(|(class, _)| *class)
        .collect::<BTreeSet<_>>();
    if new_metadata_classes != classes {
        return Err("product new metadata does not exactly cover published classes");
    }
    let class_name_rows = parts
        .runtime
        .class_names
        .iter()
        .map(|(class, name)| (*class, name.as_str()))
        .collect::<BTreeMap<_, _>>();
    if class_name_rows.len() != parts.runtime.class_names.len()
        || class_name_rows.keys().copied().collect::<BTreeSet<_>>() != classes
        || class_name_rows.iter().any(|(class, name)| {
            class_group_names
                .get(class)
                .is_none_or(|group_name| group_name != name)
        })
    {
        return Err("product class names do not exactly match published class groups");
    }
    let mut bound_classes = BTreeSet::new();
    if parts
        .runtime
        .class_value_bindings
        .iter()
        .any(|(_, binding)| !bound_classes.insert(binding.class_id))
        || bound_classes != classes
    {
        return Err("product class value bindings do not exactly cover published classes");
    }
    for (class, parameters) in &parts.runtime.class_application_parameters {
        if !valid_class(*class) {
            return Err("product class application metadata refers to an unknown class");
        }
        for parameter in parameters {
            if parameter.id.0 >= parts.next_type_param
                || parameter
                    .constraint
                    .is_some_and(|ty| !type_in_range(ty, store_len))
                || matches!(parameter.default, ClassTypeParameterDefault::Ready(ty) if !type_in_range(ty, store_len))
            {
                return Err("product class application metadata has an invalid type reference");
            }
        }
        if let Some((_, OwnedPublishedClassTerminal::Ready(surface))) = parts
            .published_types
            .classes
            .iter()
            .find(|(published, _)| published == class)
        {
            let parameter_ids = parameters
                .iter()
                .map(|parameter| parameter.id)
                .collect::<Vec<_>>();
            if parameter_ids != surface.type_params() {
                return Err(
                    "product class application parameters do not match the published class",
                );
            }
        }
    }
    for (class, metadata) in &parts.runtime.class_new_metadata {
        if !valid_class(*class) || !valid_class(metadata.ctor_declaring_class) {
            return Err("product runtime class metadata refers to an unknown class");
        }
        let mut current = *class;
        let mut visited = BTreeSet::new();
        while current != metadata.ctor_declaring_class && visited.insert(current) {
            let Some(parent) = parts
                .runtime
                .class_parents
                .iter()
                .find_map(|(child, parent)| (*child == current).then_some(*parent))
            else {
                break;
            };
            current = parent;
        }
        if current != metadata.ctor_declaring_class {
            return Err("product constructor owner is not on the class parent chain");
        }
    }
    if parts
        .runtime
        .class_parents
        .iter()
        .any(|(class, parent)| !valid_class(*class) || !valid_class(*parent))
        || parts
            .runtime
            .class_names
            .iter()
            .any(|(class, _)| !valid_class(*class))
    {
        return Err("product runtime class metadata refers to an unknown class");
    }
    if parts
        .runtime
        .class_value_aliases
        .iter()
        .chain(&parts.runtime.standalone_namespace_value_aliases)
        .any(|(alias, target)| !valid_storage(*alias) || !valid_storage(*target))
    {
        return Err("product runtime value alias is out of range");
    }
    if parts
        .runtime
        .class_value_bindings
        .iter()
        .any(|(storage, binding)| !valid_storage(*storage) || !valid_class(binding.class_id))
    {
        return Err("product class value binding has an invalid reference");
    }
    for (_, binding) in &parts.runtime.class_value_bindings {
        let Some((_, parameters)) = parts
            .runtime
            .class_application_parameters
            .iter()
            .find(|(class, _)| *class == binding.class_id)
        else {
            return Err("product class value binding has no application metadata");
        };
        let expected_generic = !parameters.is_empty();
        if binding.has_header_type_params != expected_generic {
            return Err(
                "product class value binding generic bit does not match application metadata",
            );
        }
    }
    if parts.runtime.class_value_aliases.iter().any(|(_, target)| {
        !parts
            .runtime
            .class_value_bindings
            .iter()
            .any(|(storage, _)| storage == target)
    }) {
        return Err("product class value alias does not target a class binding");
    }

    let ready_namespace_storages = parts
        .runtime
        .namespace_terminals
        .iter()
        .filter_map(|row| match row.terminal {
            FrozenNamespaceValueTerminalSnapshot::Ready { storage, .. } => Some(storage),
            FrozenNamespaceValueTerminalSnapshot::Unavailable(_) => None,
        })
        .collect::<Vec<_>>();
    for row in &parts.runtime.namespace_terminals {
        if parts.binder.namespaces.get(row.namespace).is_none() {
            return Err("product namespace terminal refers to an unknown namespace");
        }
        if let FrozenNamespaceValueTerminalSnapshot::Ready { storage, ty } = row.terminal {
            if !valid_storage(storage) || !type_in_range(ty, store_len) {
                return Err("product namespace terminal has an invalid ready reference");
            }
        }
    }
    if parts
        .runtime
        .standalone_namespace_value_aliases
        .iter()
        .any(|(_, target)| !ready_namespace_storages.contains(target))
    {
        return Err("product namespace alias does not target a ready namespace root");
    }
    if parts.runtime.named_function_symbols.iter().any(|symbol| {
        parts
            .binder
            .symbols
            .get(*symbol)
            .is_none_or(|binding| binding.function_values.is_empty())
    }) {
        return Err("product named-function metadata refers to a non-function symbol");
    }
    Ok(())
}

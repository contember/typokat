//! Measurement-only compiler for injected declaration-library profiles.

use super::classes::application::ClassTypeParameterDefault;
use super::context::{DeclTypes, TypeDecl};
use super::events_library::{
    library_record_ticket_key, LibraryEventKey, LibraryEventLedger, LibraryEventLedgerError,
    LibraryRecordTicket, LibrarySemanticReportingAdapter,
};
use super::lexical_events::LexicalReservations;
use super::lexical_events_library::library_unit;
#[cfg(test)]
use super::lexical_events_library::ExactUnit;
use super::library_identities::LibraryIdentityTerminal;
use super::library_identities::LibrarySemanticIdentities;
use super::library_reporting::LibraryReportingConsumer;
#[cfg(test)]
use super::library_reporting::LibraryReportingFamily;
#[cfg(test)]
use super::library_reporting::LibraryReportingReceipt;
#[cfg(test)]
use super::namespace_values::NamespaceValueRegistry;
use super::namespace_values::{
    FrozenNamespaceValueTerminalSnapshot, FrozenNamespaceValueTerminalSnapshotRow,
};
#[cfg(test)]
use super::replay_index::ReplayReverseEdge;
use super::replay_index::{
    admit_generated_collision_replay_index, baseline_record, AdmittedCollisionReplayIndex,
    CollisionReplayIndex, ReplayDependencyTrace, ReplayIndexAdmissionError,
    ReplayIndexAdmissionLimits, ReplayIndexGenerationError, ReplayOwner, ReplayOwnerSite,
    ReplayRootSlot,
};
use super::reporting_record::CheckerRecord;
use super::type_groups::{
    PublishedTypeEnvironment, PublishedTypeGroupSurface, PublishedTypeGroupTerminal,
    PublishedTypeParameterDefault,
};
use super::{
    build_pass_with_tickets, finish_semantic_effects, reserve_type_decls,
    FrozenCheckerRuntimeMetadata, FrozenCheckerRuntimeSnapshotParts, PassReporting,
    PassReportingPlan,
};
#[cfg(test)]
use super::{check_bound_user_program_with_final_identity_inspector, BoundUserBase};
#[cfg(test)]
use crate::binder::bind::LibraryBinderCheckpointEnds;
use crate::binder::bind::{LibraryBinderCheckpoint, LibraryBinderUnit, ProjectBinderBuilder};
#[cfg(test)]
use crate::binder::declaration::DeclId;
#[cfg(test)]
use crate::binder::declaration::TypeFragmentKind;
use crate::binder::declaration::{
    source_global_binding_census_with_provenance, SourceBindingSlot, SourceGlobalContributorKind,
    TypeGroupId, ValueStorageId,
};
#[cfg(test)]
use crate::binder::namespace::MergeDeclarationKind;
use crate::binder::namespace::{
    exact_key, source_file_kind, CompilationUnit, ExactKey, ExportContextKind,
    ExportSyntaxDisposition, ModuleBindingContext, NamespaceId, SourceFileKind,
};
use crate::binder::roots::{collect_root_rows, RootNameRow};
use crate::binder::scope::ScopeId;
#[cfg(test)]
use crate::binder::symbol::SymbolId;
use crate::binder::Binder;
use crate::class_semantics::CanonicalPublishedClassTerminal;
#[cfg(test)]
use crate::class_semantics::DemandOutcome;
use crate::class_semantics::PublishedClassSnapshotTerminal;
#[cfg(test)]
use crate::diagnostics::render_type;
use crate::diagnostics::{render_to_writer_with_format, DiagnosticFormat};
use crate::source::{CompilationOrigin, LibraryFileOrdinal};
use crate::span::Span;
use crate::types::repr::ClassId;
use crate::types::repr::TypeParamId;
#[cfg(test)]
use crate::types::repr::{
    IntrinsicKind, LiteralValue, ModifierOp, ObjectType, TypeTag, Visibility,
};
#[cfg(test)]
use crate::types::store::Store;
use crate::types::store::TypeId;
use crate::types::Interner;
use oxc_allocator::Allocator;
use oxc_parser::{Parser, ParserReturn};
use oxc_span::SourceType;
#[cfg(test)]
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
#[cfg(test)]
use std::time::{Duration, Instant};

pub(crate) struct OwnedLibraryRuntimeState {
    interner: Interner,
    binder: Binder,
    published_types: PublishedTypeEnvironment,
    decl_types: DeclTypes,
    semantic_identities: Option<LibrarySemanticIdentities>,
    runtime: FrozenCheckerRuntimeMetadata,
    next_type_param: u32,
    next_class_id: u32,
    source_file_count: u32,
    replay_index: Option<Box<AdmittedCollisionReplayIndex>>,
}

pub(in crate::check::checker) struct OwnedLibraryRuntimeSnapshotParts {
    pub(in crate::check::checker) interner: Interner,
    pub(in crate::check::checker) binder: Binder,
    pub(in crate::check::checker) published_types:
        super::type_groups::PublishedTypeEnvironmentSnapshotParts,
    pub(in crate::check::checker) decl_types: Vec<Option<TypeId>>,
    pub(in crate::check::checker) semantic_identities:
        Option<super::library_identities::LibrarySemanticIdentitiesSnapshotParts>,
    pub(in crate::check::checker) runtime: super::FrozenCheckerRuntimeSnapshotParts,
    pub(in crate::check::checker) next_type_param: u32,
    pub(in crate::check::checker) next_class_id: u32,
    pub(in crate::check::checker) source_file_count: u32,
}

pub(crate) struct CompiledLibraryRuntimeProduct {
    pub(in crate::check::checker) _parts: OwnedLibraryRuntimeSnapshotParts,
    pub(crate) _replay_index: AdmittedCollisionReplayIndex,
}

pub(crate) fn freeze_library_runtime_product(
    mut state: OwnedLibraryRuntimeState,
) -> Result<CompiledLibraryRuntimeProduct, &'static str> {
    let replay_index = state
        .replay_index
        .take()
        .ok_or("source library compiler did not produce a replay index")?;
    state
        .into_snapshot_parts()
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
        return Err("snapshot library state has no source files");
    }
    let mut source_keys = binder
        .snapshot_module_sources()
        .values()
        .map(|source| source.0)
        .collect::<Vec<_>>();
    if source_keys.is_empty() {
        return Err("snapshot binder has no retained source ownership");
    }
    source_keys.sort_unstable();
    if source_keys.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("snapshot binder repeats a retained source key");
    }
    let expected_len = source_file_count
        .checked_add(1)
        .ok_or("snapshot source count overflows the prelude prefix")?;
    if u32::try_from(source_keys.len()).map_err(|_| "snapshot source-key count does not fit u32")?
        != expected_len
        || source_keys
            .iter()
            .enumerate()
            .any(|(index, source)| *source != u32::try_from(index).unwrap_or(u32::MAX))
    {
        return Err("snapshot binder source keys are not the contiguous prelude/library prefix");
    }
    if binder
        .snapshot_module_sources()
        .keys()
        .any(|scope| binder.graph.get(*scope).is_none())
    {
        return Err("snapshot binder source ownership refers to an unknown scope");
    }
    Ok(())
}

fn validate_owned_library_snapshot_parts(
    parts: &OwnedLibraryRuntimeSnapshotParts,
) -> Result<(), &'static str> {
    fn type_in_range(id: TypeId, store_len: usize) -> bool {
        usize::try_from(id.0).is_ok_and(|index| index < store_len)
    }

    fn type_param_precedes_counter(id: TypeParamId, next_type_param: u32) -> bool {
        id.0 < next_type_param
    }

    validate_library_source_prefix(&parts.binder, parts.source_file_count)?;
    if parts.published_types.groups.len() != parts.binder.type_groups.len() {
        return Err("snapshot published type-group count does not match the binder");
    }
    if parts.decl_types.len()
        != usize::try_from(parts.binder.decl_count)
            .map_err(|_| "snapshot binder storage count does not fit usize")?
    {
        return Err("snapshot declaration types do not cover the binder storage prefix");
    }

    let store = parts.interner.store();
    let store_len = store.len();
    if parts
        .decl_types
        .iter()
        .flatten()
        .any(|ty| !type_in_range(*ty, store_len))
    {
        return Err("snapshot declaration type is out of range");
    }

    let mut classes = BTreeSet::new();
    for (class, terminal) in &parts.published_types.classes {
        if !classes.insert(*class) {
            return Err("snapshot class publication repeats a class id");
        }
        if class.0 >= parts.next_class_id {
            return Err("snapshot class id collides with the next class counter");
        }
        let PublishedClassSnapshotTerminal::Ready(surface) = terminal else {
            continue;
        };
        if surface
            .type_params()
            .iter()
            .any(|parameter| !type_param_precedes_counter(*parameter, parts.next_type_param))
        {
            return Err("snapshot type parameter id collides with the next parameter counter");
        }
        if surface.class() != *class
            || !type_in_range(surface.instance_template(), store_len)
            || !type_in_range(surface.static_template(), store_len)
            || surface
                .constructor_template()
                .is_some_and(|ty| !type_in_range(ty, store_len))
        {
            return Err("snapshot published class surface has an invalid reference");
        }
    }

    let mut class_group_names = BTreeMap::new();
    for (index, terminal) in parts.published_types.groups.iter().enumerate() {
        let PublishedTypeGroupTerminal::Ready(group) = terminal else {
            continue;
        };
        let group_id = TypeGroupId(
            u32::try_from(index).map_err(|_| "snapshot type-group index does not fit u32")?,
        );
        if parts
            .binder
            .type_groups
            .get(group_id)
            .is_none_or(|binding| binding.name != group.name)
        {
            return Err("snapshot published type-group name does not match the binder");
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
            return Err("snapshot published type group has an invalid type reference");
        }
        match group.surface {
            PublishedTypeGroupSurface::Template(ty) if !type_in_range(ty, store_len) => {
                return Err("snapshot published type-group template is out of range")
            }
            PublishedTypeGroupSurface::Class(class) => {
                if !classes.contains(&class) {
                    return Err("snapshot published type group refers to an unknown class");
                }
                if class_group_names
                    .insert(class, group.name.as_str())
                    .is_some()
                {
                    return Err("snapshot class identity is published by multiple type groups");
                }
            }
            PublishedTypeGroupSurface::Template(_) => {}
        }
    }
    if class_group_names.keys().copied().collect::<BTreeSet<_>>() != classes {
        return Err("snapshot published classes do not have exact type-group ownership");
    }

    for index in 0..store_len {
        let ty = TypeId(
            u32::try_from(index).map_err(|_| "snapshot type-store length does not fit TypeId")?,
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
            return Err("snapshot type parameter id collides with the next parameter counter");
        }
        if store.class_instance_type(ty).is_some_and(|instance| {
            instance.class.0 >= parts.next_class_id || !classes.contains(&instance.class)
        }) {
            return Err("snapshot class instance refers to an unknown class identity");
        }
        if store.object_type(ty).is_some_and(|object| {
            object.properties.iter().any(|property| {
                property.declaring_class.is_some_and(|class| {
                    class.0 >= parts.next_class_id || !classes.contains(&class)
                })
            })
        }) {
            return Err("snapshot object property refers to an unknown class identity");
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
                return Err("snapshot installed semantic identities are not all ready");
            };
            if !type_in_range(identity.template, store_len)
                || identity
                    .parameters
                    .iter()
                    .any(|parameter| parameter.0 >= parts.next_type_param)
            {
                return Err("snapshot semantic identity has an invalid type reference");
            }
            let Some(PublishedTypeGroupTerminal::Ready(group)) =
                parts.published_types.groups.get(identity.group.index())
            else {
                return Err("snapshot semantic identity refers to an unpublished type group");
            };
            if group.name != expected_name
                || group.parameters != identity.parameters
                || group.surface != PublishedTypeGroupSurface::Template(identity.template)
            {
                return Err("snapshot semantic identity does not match its published type group");
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
        return Err("snapshot class application metadata does not exactly cover published classes");
    }
    let new_metadata_classes = parts
        .runtime
        .class_new_metadata
        .iter()
        .map(|(class, _)| *class)
        .collect::<BTreeSet<_>>();
    if new_metadata_classes != classes {
        return Err("snapshot new metadata does not exactly cover published classes");
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
        return Err("snapshot class names do not exactly match published class groups");
    }
    let mut bound_classes = BTreeSet::new();
    if parts
        .runtime
        .class_value_bindings
        .iter()
        .any(|(_, binding)| !bound_classes.insert(binding.class_id))
        || bound_classes != classes
    {
        return Err("snapshot class value bindings do not exactly cover published classes");
    }
    for (class, parameters) in &parts.runtime.class_application_parameters {
        if !valid_class(*class) {
            return Err("snapshot class application metadata refers to an unknown class");
        }
        for parameter in parameters {
            if parameter.id.0 >= parts.next_type_param
                || parameter
                    .constraint
                    .is_some_and(|ty| !type_in_range(ty, store_len))
                || matches!(parameter.default, ClassTypeParameterDefault::Ready(ty) if !type_in_range(ty, store_len))
            {
                return Err("snapshot class application metadata has an invalid type reference");
            }
        }
        if let Some((_, PublishedClassSnapshotTerminal::Ready(surface))) = parts
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
                    "snapshot class application parameters do not match the published class",
                );
            }
        }
    }
    for (class, metadata) in &parts.runtime.class_new_metadata {
        if !valid_class(*class) || !valid_class(metadata.ctor_declaring_class) {
            return Err("snapshot runtime class metadata refers to an unknown class");
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
            return Err("snapshot constructor owner is not on the class parent chain");
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
        return Err("snapshot runtime class metadata refers to an unknown class");
    }
    if parts
        .runtime
        .class_value_aliases
        .iter()
        .chain(&parts.runtime.standalone_namespace_value_aliases)
        .any(|(alias, target)| !valid_storage(*alias) || !valid_storage(*target))
    {
        return Err("snapshot runtime value alias is out of range");
    }
    if parts
        .runtime
        .class_value_bindings
        .iter()
        .any(|(storage, binding)| !valid_storage(*storage) || !valid_class(binding.class_id))
    {
        return Err("snapshot class value binding has an invalid reference");
    }
    for (_, binding) in &parts.runtime.class_value_bindings {
        let Some((_, parameters)) = parts
            .runtime
            .class_application_parameters
            .iter()
            .find(|(class, _)| *class == binding.class_id)
        else {
            return Err("snapshot class value binding has no application metadata");
        };
        let expected_generic = !parameters.is_empty();
        if binding.has_header_type_params != expected_generic {
            return Err(
                "snapshot class value binding generic bit does not match application metadata",
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
        return Err("snapshot class value alias does not target a class binding");
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
            return Err("snapshot namespace terminal refers to an unknown namespace");
        }
        if let FrozenNamespaceValueTerminalSnapshot::Ready { storage, ty } = row.terminal {
            if !valid_storage(storage) || !type_in_range(ty, store_len) {
                return Err("snapshot namespace terminal has an invalid ready reference");
            }
        }
    }
    if parts
        .runtime
        .standalone_namespace_value_aliases
        .iter()
        .any(|(_, target)| !ready_namespace_storages.contains(target))
    {
        return Err("snapshot namespace alias does not target a ready namespace root");
    }
    if parts.runtime.named_function_symbols.iter().any(|symbol| {
        parts
            .binder
            .symbols
            .get(*symbol)
            .is_none_or(|binding| binding.function_values.is_empty())
    }) {
        return Err("snapshot named-function metadata refers to a non-function symbol");
    }
    Ok(())
}

impl OwnedLibraryRuntimeState {
    #[cfg(test)]
    pub(in crate::check::checker) fn into_user_project_base(
        self,
    ) -> (Interner, Binder, super::BoundUserBase) {
        let Self {
            interner,
            binder,
            published_types,
            decl_types,
            semantic_identities,
            runtime,
            next_type_param,
            next_class_id,
            source_file_count: _,
            replay_index: _,
        } = self;
        (
            interner,
            binder,
            super::BoundUserBase {
                published_types,
                library_semantic_identities: semantic_identities,
                lexical_array_alias: None,
                decl_types,
                next_type_param,
                next_class_id,
                runtime,
            },
        )
    }

    pub(crate) fn freeze_as_library_base(&mut self) -> Result<(), &'static str> {
        self.interner.freeze_as_base()?;
        self.binder.freeze_as_base()?;
        self.published_types.freeze_as_base()?;
        self.decl_types.freeze_as_base()?;
        self.runtime.freeze_as_base()
    }

    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "activated by the WU4 user-delta entry point")
    )]
    pub(crate) fn fork_collision_free_user_delta(
        &self,
        _capability: crate::library::CollisionFreeUserDeltaCapability,
    ) -> Result<Self, &'static str> {
        self.fork_user_delta()
    }

    fn fork_user_delta(&self) -> Result<Self, &'static str> {
        #[cfg(test)]
        USER_DELTA_FORKS.set(USER_DELTA_FORKS.get().saturating_add(1));
        Ok(Self {
            interner: self.interner.fork_delta()?,
            binder: self.binder.fork_delta()?,
            published_types: self.published_types.fork_delta()?,
            decl_types: self.decl_types.fork_delta()?,
            semantic_identities: self.semantic_identities.clone(),
            runtime: self.runtime.fork_delta()?,
            next_type_param: self.next_type_param,
            next_class_id: self.next_class_id,
            source_file_count: self.source_file_count,
            replay_index: None,
        })
    }

    #[cfg(test)]
    fn fork_user_delta_for_test(&self) -> Result<Self, &'static str> {
        self.fork_user_delta()
    }

    #[cfg(test)]
    fn shares_checker_base_with(&self, other: &Self) -> bool {
        self.published_types
            .shares_base_with(&other.published_types)
            && self.decl_types.shares_base_with(&other.decl_types)
            && self.runtime.shares_base_with(&other.runtime)
            && match (&self.semantic_identities, &other.semantic_identities) {
                (Some(left), Some(right)) => left.shares_storage_with(right),
                (None, None) => true,
                (Some(_), None) | (None, Some(_)) => false,
            }
    }

    /// Reference-row counts of the frozen base: store, interner identity, binder.
    ///
    /// A user delta that leaked a row into the base would move one of these.
    #[cfg(test)]
    pub(crate) fn reference_record_counts_for_test(&self) -> [usize; 3] {
        let (store, interner) = self.interner.snapshot_reference_records_for_test();
        let binder = crate::binder::snapshot::snapshot_reference_records(&self.binder)
            .expect("frozen binder projects its reference rows")
            .len();
        [store.len(), interner.len(), binder]
    }

    #[cfg(test)]
    pub(crate) fn storage_identity_for_test(&self) -> [usize; 8] {
        [
            std::ptr::from_ref(self).addr(),
            std::ptr::from_ref(&self.interner).addr(),
            std::ptr::from_ref(self.interner.store()).addr(),
            std::ptr::from_ref(&self.binder).addr(),
            std::ptr::from_ref(&self.published_types).addr(),
            std::ptr::from_ref(&self.decl_types).addr(),
            std::ptr::from_ref(&self.runtime).addr(),
            self.semantic_identities
                .as_ref()
                .map_or(0, |identities| std::ptr::from_ref(identities).addr()),
        ]
    }

    #[cfg(test)]
    pub(crate) fn initial_visible_user_names_for_test(&self) -> BTreeSet<String> {
        self.binder.local_names_for_test()
    }

    #[cfg(test)]
    pub(crate) fn install_user_delta_drop_witness_for_test(
        &mut self,
        discarded: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) {
        self.interner
            .install_user_delta_drop_witness_for_test(discarded);
    }

    /// Frozen id prefixes in `FrozenLibraryPrefixes` order: types, type params, classes,
    /// scopes, symbols, declarations, type groups, namespaces, value storages.
    pub(crate) fn library_prefixes(&self) -> Result<[usize; 9], &'static str> {
        Ok([
            self.interner.store().len(),
            usize::try_from(self.next_type_param)
                .map_err(|_| "type parameter end exceeds usize")?,
            usize::try_from(self.next_class_id).map_err(|_| "class end exceeds usize")?,
            self.binder.graph.snapshot_len(),
            self.binder.symbols.len(),
            self.binder.declarations.len(),
            self.binder.type_groups.len(),
            self.binder.namespaces.len(),
            self.decl_types.len(),
        ])
    }

    #[cfg(test)]
    pub(crate) fn identity_ends_for_test(&self) -> OwnedBaseFinalIdentityEnds {
        OwnedBaseFinalIdentityEnds {
            store: self.interner.store().len(),
            declared_recipes: self.interner.store().snapshot_declared_recipes().count(),
            type_params: usize::try_from(self.next_type_param)
                .expect("type parameter end fits usize"),
            classes: usize::try_from(self.next_class_id).expect("class end fits usize"),
            scopes: self.binder.graph.snapshot_len(),
            symbols: self.binder.symbols.len(),
            declarations: self.binder.declarations.len(),
            type_groups: self.binder.type_groups.len(),
            namespaces: self.binder.namespaces.len(),
            value_storages: self.decl_types.len(),
        }
    }

    #[cfg(test)]
    pub(crate) fn named_type_for_test(&self, name: &str) -> Option<TypeId> {
        let group = self
            .binder
            .resolve_type(self.binder.compilation_global, name)
            .and_then(|symbol| self.binder.symbols.get(symbol))?
            .ty?;
        let PublishedTypeGroupTerminal::Ready(group) = self.published_types.groups().get(group)?
        else {
            return None;
        };
        match group.surface {
            PublishedTypeGroupSurface::Template(ty) => Some(ty),
            PublishedTypeGroupSurface::Class(class) => {
                match self.published_types.classes().published_class(class) {
                    DemandOutcome::Ready(surface) => Some(surface.instance_template()),
                    DemandOutcome::Exhausted(_) => None,
                }
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn frozen_structural_object_probe_for_test(&self) -> Option<(TypeId, ObjectType)> {
        self.interner.frozen_structural_object_probe_for_test()
    }

    #[cfg(test)]
    pub(crate) fn reintern_structural_type_for_test(
        &self,
        descriptor: ObjectType,
    ) -> Result<(TypeId, usize), &'static str> {
        let mut delta = self.fork_user_delta_for_test()?;
        let before = delta.interner.store().len();
        let resolved = delta.interner.intern_object(descriptor);
        Ok((resolved, delta.interner.store().len() - before))
    }

    #[cfg(test)]
    fn final_base_family_clone_counts_for_test(
        &self,
        pass: &super::context::Pass<'_, '_>,
        final_next_class_id: u32,
    ) -> BTreeMap<&'static str, u64> {
        let store = self
            .interner
            .store()
            .base_family_sharing_with(pass.interner.store());
        let interner = self.interner.base_index_family_sharing_with(pass.interner);
        let binder = self.binder.base_family_sharing_with(pass.binder);
        let published = self
            .published_types
            .base_family_sharing_with(pass.type_environment.published());
        let identities = match (
            self.semantic_identities.as_ref(),
            pass.library_semantic_identities.as_ref(),
        ) {
            (Some(base), Some(final_state)) => base.shares_storage_with(final_state),
            (None, None) => true,
            (Some(_), None) | (None, Some(_)) => false,
        };
        let runtime = &self.runtime;
        let checks = [
            ("store.rows", store[0]),
            ("store.payload-tables", store[1]),
            ("store.type-param-constraints", store[2]),
            ("store.frozen-type-params", store[3]),
            ("store.template-names", store[4]),
            ("interner.dedup-buckets", interner[0]),
            ("interner.reserved-terminals", interner[1]),
            ("interner.declared-recipes", interner[2]),
            ("interner.well-known", interner[3]),
            ("binder.scopes", binder[0]),
            ("binder.symbols", binder[1]),
            ("binder.declarations", binder[2]),
            ("binder.declaration-site-index", binder[3]),
            ("binder.type-groups", binder[4]),
            ("binder.namespaces", binder[5]),
            ("binder.namespace-indexes", binder[6]),
            ("binder.module-sources", binder[7]),
            (
                "decl-types.slots",
                self.decl_types.shares_base_with(&pass.decl_types),
            ),
            ("published-types.groups", published[0]),
            ("published-types.classes", published[1]),
            (
                "namespace-terminals",
                pass.namespace_values
                    .terminals_share_base_with(&runtime.namespace_terminals),
            ),
            (
                "function-groups.symbols",
                runtime
                    .named_function_symbols
                    .shares_base_with(&pass.named_function_symbols),
            ),
            (
                "class.application-parameters",
                runtime
                    .class_application_parameters
                    .shares_base_with(&pass.class_application_parameters),
            ),
            ("class.parameter-defaults", published[0]),
            (
                "class.parents",
                runtime.class_parents.shares_base_with(&pass.class_parents),
            ),
            (
                "class.names",
                runtime.class_names.shares_base_with(&pass.class_names),
            ),
            (
                "class.new-metadata",
                runtime
                    .class_new_metadata
                    .shares_base_with(&pass.class_new_metadata),
            ),
            (
                "class.value-identities",
                runtime
                    .class_value_bindings
                    .shares_base_with(&pass.class_value_bindings),
            ),
            (
                "class.aliases",
                runtime
                    .class_value_aliases
                    .shares_base_with(&pass.class_value_aliases)
                    && runtime
                        .standalone_namespace_value_aliases
                        .shares_base_with(&pass.standalone_namespace_value_aliases),
            ),
            ("semantic-identities", identities),
            (
                "next-ids",
                pass.next_type_param >= self.next_type_param
                    && final_next_class_id >= self.next_class_id,
            ),
        ];
        checks
            .into_iter()
            .map(|(family, shared)| (family, u64::from(!shared)))
            .collect()
    }

    #[cfg(test)]
    fn final_local_rows_written_for_test(pass: &super::context::Pass<'_, '_>) -> u64 {
        let store = pass.interner.store().local_family_row_counts_for_test();
        let interner = pass.interner.local_index_row_counts_for_test();
        let binder = pass.binder.local_family_row_counts_for_test();
        let published = pass
            .type_environment
            .published()
            .local_family_row_counts_for_test();
        let rows = store.into_iter().sum::<usize>()
            + interner.into_iter().sum::<usize>()
            + binder.into_iter().sum::<usize>()
            + pass.decl_types.local_len()
            + published.into_iter().sum::<usize>()
            + pass.namespace_values.local_terminal_row_count_for_test()
            + pass.named_function_symbols.local_len()
            + pass.class_application_parameters.local_len()
            + pass.class_parents.local_len()
            + pass.class_names.local_len()
            + pass.class_new_metadata.local_len()
            + pass.class_value_bindings.local_len()
            + pass.class_value_aliases.local_len()
            + pass.standalone_namespace_value_aliases.local_len();
        u64::try_from(rows).expect("local row count fits u64")
    }

    #[cfg(test)]
    pub(crate) fn type_count(&self) -> usize {
        self.interner.store().len()
    }

    pub(in crate::check::checker) fn into_snapshot_parts(
        self,
    ) -> Result<OwnedLibraryRuntimeSnapshotParts, &'static str> {
        if self
            .semantic_identities
            .as_ref()
            .is_some_and(|identities| !identities.all_ready())
        {
            return Err("snapshot installed semantic identities are not all ready");
        }
        let parts = OwnedLibraryRuntimeSnapshotParts {
            interner: self.interner,
            binder: self.binder,
            published_types: self.published_types.snapshot_parts()?,
            decl_types: self.decl_types.snapshot_slots(),
            semantic_identities: self
                .semantic_identities
                .as_ref()
                .map(super::library_identities::LibrarySemanticIdentities::snapshot_parts),
            runtime: self.runtime.snapshot_parts()?,
            next_type_param: self.next_type_param,
            next_class_id: self.next_class_id,
            source_file_count: self.source_file_count,
        };
        validate_owned_library_snapshot_parts(&parts)?;
        Ok(parts)
    }

    pub(crate) fn replay_index(&self) -> Option<&AdmittedCollisionReplayIndex> {
        self.replay_index.as_deref()
    }

    /// Restore a runtime from its own snapshot parts; the base it yields carries no replay index.
    #[cfg(test)]
    pub(in crate::check::checker) fn from_snapshot_parts(
        parts: OwnedLibraryRuntimeSnapshotParts,
    ) -> Result<Self, &'static str> {
        validate_owned_library_snapshot_parts(&parts)?;
        let decl_types = DeclTypes::from_snapshot_slots(parts.decl_types, parts.binder.decl_count)?;
        let semantic_identities = parts
            .semantic_identities
            .map(super::library_identities::LibrarySemanticIdentities::from_snapshot_parts)
            .transpose()?;
        if semantic_identities
            .as_ref()
            .is_some_and(|identities| !identities.all_ready())
        {
            return Err("snapshot installed semantic identities are not all ready");
        }
        Ok(Self {
            interner: parts.interner,
            binder: parts.binder,
            published_types: PublishedTypeEnvironment::from_snapshot_parts(parts.published_types)?,
            decl_types,
            semantic_identities,
            runtime: FrozenCheckerRuntimeMetadata::from_snapshot_parts(parts.runtime)?,
            next_type_param: parts.next_type_param,
            next_class_id: parts.next_class_id,
            source_file_count: parts.source_file_count,
            replay_index: None,
        })
    }

    /// Root names published into the compilation-global scope of the compiled library.
    pub(crate) fn library_root_names(&self) -> Result<BTreeSet<String>, &'static str> {
        collect_root_rows(&self.binder)
            .map(|rows| rows.into_iter().map(|row| row.name).collect())
            .map_err(|_| "library binder does not expose a compilation-global root index")
    }
}

#[cfg(test)]
pub(crate) fn compile_synthetic_padding_base_for_test(
    namespace_count: usize,
) -> Result<OwnedLibraryRuntimeState, String> {
    let mut source = String::new();
    for index in 0..namespace_count {
        source.push_str(&format!(
            "declare namespace Pad{index} {{\n\
             export class Box<T> {{ value: T; }}\n\
             export interface Shape {{ value: string; }}\n\
             export const value: string;\n\
             }}\n"
        ));
    }
    let injected = [InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "synthetic-padding.d.ts",
        source: &source,
    }];
    let (_, mut state) = compile_owned_injected_profile(&injected)
        .map_err(|error| format!("synthetic padding base failed: {error:?}"))?;
    state.freeze_as_library_base().map_err(str::to_owned)?;
    Ok(state)
}

#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct OwnedBaseUserTimings {
    pub(crate) parse: Duration,
    pub(crate) bind: Duration,
    pub(crate) check: Duration,
}

#[cfg(test)]
pub(crate) struct OwnedBaseUserRun {
    pub(crate) result: super::CheckResult,
    pub(crate) timings: OwnedBaseUserTimings,
    pub(crate) witness: OwnedBaseContinuationWitness,
    pub(crate) final_identity: OwnedBaseFinalIdentityWitness,
}

#[cfg(test)]
pub(crate) struct OwnedBaseUserProjectRun {
    pub(crate) reports: Vec<crate::driver::FileReport>,
    pub(crate) final_identity: OwnedBaseFinalIdentityWitness,
    pub(crate) cross_file: OwnedBaseCrossFileWitness,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OwnedBaseCrossFileWitness {
    pub(crate) producer_type_group: TypeGroupId,
    pub(crate) consumer_type_group: TypeGroupId,
    pub(crate) producer_value_storage: ValueStorageId,
    pub(crate) consumer_value_storage: ValueStorageId,
}

#[cfg(test)]
thread_local! {
    static USER_SOURCE_PARSE_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static USER_SOURCE_BIND_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static USER_SOURCE_CHECK_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static USER_DELTA_FORKS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) struct UserDeltaForkScopeForTest(u64);

#[cfg(test)]
impl UserDeltaForkScopeForTest {
    pub(crate) fn start() -> Self {
        Self(USER_DELTA_FORKS.get())
    }

    pub(crate) fn finish(self) -> u64 {
        USER_DELTA_FORKS.get().saturating_sub(self.0)
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct UserSourceWorkForTest {
    pub(crate) parses: u64,
    pub(crate) binds: u64,
    pub(crate) checks: u64,
}

#[cfg(test)]
fn user_source_work_for_test() -> UserSourceWorkForTest {
    UserSourceWorkForTest {
        parses: USER_SOURCE_PARSE_CALLS.get(),
        binds: USER_SOURCE_BIND_CALLS.get(),
        checks: USER_SOURCE_CHECK_CALLS.get(),
    }
}

#[cfg(test)]
pub(crate) fn record_user_source_parses_for_test(count: usize) {
    USER_SOURCE_PARSE_CALLS.set(
        USER_SOURCE_PARSE_CALLS
            .get()
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX)),
    );
}

#[cfg(test)]
pub(in crate::check::checker) fn record_user_source_binds_for_test(count: usize) {
    USER_SOURCE_BIND_CALLS.set(
        USER_SOURCE_BIND_CALLS
            .get()
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX)),
    );
}

#[cfg(test)]
pub(in crate::check::checker) fn record_user_source_checks_for_test(count: usize) {
    USER_SOURCE_CHECK_CALLS.set(
        USER_SOURCE_CHECK_CALLS
            .get()
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX)),
    );
}

#[cfg(test)]
pub(crate) struct UserSourceWorkScopeForTest(UserSourceWorkForTest);

#[cfg(test)]
impl UserSourceWorkScopeForTest {
    pub(crate) fn start() -> Self {
        Self(user_source_work_for_test())
    }

    pub(crate) fn finish(self) -> UserSourceWorkForTest {
        let end = user_source_work_for_test();
        UserSourceWorkForTest {
            parses: end.parses.saturating_sub(self.0.parses),
            binds: end.binds.saturating_sub(self.0.binds),
            checks: end.checks.saturating_sub(self.0.checks),
        }
    }
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OwnedBaseFinalIdentityWitness {
    pub(crate) ends: OwnedBaseFinalIdentityEnds,
    pub(crate) actual_ids: OwnedBaseActualIds,
    pub(crate) named_alias_types: BTreeMap<String, TypeId>,
    pub(crate) local_names: BTreeSet<String>,
    pub(crate) references: OwnedBaseReferenceSummary,
    pub(crate) reused_base_shape: Option<OwnedBaseReusedShapeWitness>,
    pub(crate) base_row_clone_counts: BTreeMap<&'static str, u64>,
    pub(crate) local_rows_written: u64,
}

#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct OwnedBaseActualIds {
    pub(crate) types: Vec<usize>,
    pub(crate) type_params: Vec<usize>,
    pub(crate) classes: Vec<usize>,
    pub(crate) scopes: Vec<usize>,
    pub(crate) symbols: Vec<usize>,
    pub(crate) declarations: Vec<usize>,
    pub(crate) type_groups: Vec<usize>,
    pub(crate) namespaces: Vec<usize>,
    pub(crate) value_storages: Vec<usize>,
}

#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct OwnedBaseReferenceSummary {
    pub(crate) base_to_delta: u64,
    pub(crate) delta_to_base: u64,
    pub(crate) delta_to_delta: u64,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OwnedBaseFinalIdentityEnds {
    pub(crate) store: usize,
    pub(crate) declared_recipes: usize,
    pub(crate) type_params: usize,
    pub(crate) classes: usize,
    pub(crate) scopes: usize,
    pub(crate) symbols: usize,
    pub(crate) declarations: usize,
    pub(crate) type_groups: usize,
    pub(crate) namespaces: usize,
    pub(crate) value_storages: usize,
}

#[cfg(test)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct OwnedBaseReusedShapeWitness {
    pub(crate) type_id: TypeId,
    pub(crate) tag: TypeTag,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OwnedBaseContinuationWitness {
    pub(crate) base_store_len: usize,
    pub(crate) final_store_len: usize,
    pub(crate) base_type_group_count: usize,
    pub(crate) final_type_group_count: usize,
    pub(crate) base_decl_count: u32,
    pub(crate) final_decl_count: u32,
    pub(crate) source_key: u32,
    pub(crate) base_max_source_key: u32,
    pub(crate) array_group_stable: bool,
    pub(crate) document_value_stable: bool,
    pub(crate) store_prefix_stable: Option<bool>,
}

#[cfg(test)]
fn store_prefix_digest(
    store: &Store,
    type_len: usize,
    declared_recipe_len: usize,
) -> Result<String, String> {
    let mut bytes = CanonicalBytes::domain(b"typokat-owned-base-store-prefix-v1");
    bytes
        .usize(declared_recipe_len)
        .map_err(|error| format!("{error:?}"))?;
    for index in 0..declared_recipe_len {
        let recipe = crate::types::repr::DeclaredRecipeId(
            u32::try_from(index).map_err(|_| "declared recipe prefix overflow")?,
        );
        encode_declared_recipe_row(&mut bytes, store, recipe)
            .map_err(|error| format!("{error:?}"))?;
    }
    for index in 0..type_len {
        let id = TypeId(u32::try_from(index).map_err(|_| "type id prefix overflow")?);
        encode_store_row(&mut bytes, store, id).map_err(|error| format!("{error:?}"))?;
    }
    Ok(format!("{:x}", Sha256::digest(bytes.finish())))
}

#[cfg(test)]
struct FinalIdentityInspection<'a> {
    base_store_len: usize,
    base_value_storage_len: usize,
    base_type_group_len: usize,
    base_namespace_len: usize,
    binder: &'a Binder,
    published: &'a PublishedTypeEnvironment,
    interner: &'a Interner,
    decl_types: &'a DeclTypes,
    next_type_param: u32,
    next_class_id: u32,
    actual_class_ids: Vec<ClassId>,
    references: OwnedBaseReferenceSummary,
    base_row_clone_counts: BTreeMap<&'static str, u64>,
    local_rows_written: u64,
}

#[cfg(test)]
fn base_domain_limit(ends: &OwnedBaseFinalIdentityEnds, domain: u8) -> Option<usize> {
    match domain {
        1 => Some(ends.store),
        2 => Some(ends.type_params),
        3 => Some(ends.classes),
        4 => Some(ends.scopes),
        5 => Some(ends.symbols),
        6 => Some(ends.declarations),
        7 => Some(ends.type_groups),
        8 => Some(ends.namespaces),
        9 => Some(ends.value_storages),
        10 => Some(ends.declared_recipes),
        _ => None,
    }
}

#[cfg(test)]
fn classify_live_reference(
    summary: &mut OwnedBaseReferenceSummary,
    base: &OwnedBaseFinalIdentityEnds,
    owner_domain: u8,
    owner: u32,
    target_domain: u8,
    target: u32,
) {
    let Some(target_limit) = base_domain_limit(base, target_domain) else {
        return;
    };
    let owner_is_base = base_domain_limit(base, owner_domain)
        .is_some_and(|owner_limit| usize::try_from(owner).is_ok_and(|owner| owner < owner_limit));
    let target_is_base = usize::try_from(target).is_ok_and(|target| target < target_limit);
    match (owner_is_base, target_is_base) {
        (true, false) => summary.base_to_delta += 1,
        (false, true) => summary.delta_to_base += 1,
        (false, false) => summary.delta_to_delta += 1,
        (true, true) => {}
    }
}

#[cfg(test)]
fn final_reference_summary(
    pass: &super::context::Pass<'_, '_>,
    base: &OwnedBaseFinalIdentityEnds,
) -> OwnedBaseReferenceSummary {
    let mut summary = OwnedBaseReferenceSummary::default();
    for (owner_domain, target_domain, _, owner, target) in
        pass.interner.local_type_reference_records_for_test()
    {
        classify_live_reference(
            &mut summary,
            base,
            owner_domain,
            owner,
            target_domain,
            target,
        );
    }

    for (owner_domain, owner, target_domain, target) in
        pass.binder.local_reference_records_for_test()
    {
        classify_live_reference(
            &mut summary,
            base,
            owner_domain,
            owner,
            target_domain,
            target,
        );
    }

    for (owner, ty) in pass.decl_types.local_slots() {
        classify_live_reference(&mut summary, base, 9, owner.0, 9, owner.0);
        if let Some(ty) = ty {
            classify_live_reference(&mut summary, base, 9, owner.0, 1, ty.0);
        }
    }

    let published = pass.type_environment.published();
    for (owner, terminal) in published.local_group_terminals() {
        classify_live_reference(&mut summary, base, 7, owner.0, 7, owner.0);
        let PublishedTypeGroupTerminal::Ready(group) = terminal else {
            continue;
        };
        match group.surface {
            PublishedTypeGroupSurface::Template(ty) => {
                classify_live_reference(&mut summary, base, 7, owner.0, 1, ty.0)
            }
            PublishedTypeGroupSurface::Class(class) => {
                classify_live_reference(&mut summary, base, 7, owner.0, 3, class.0)
            }
        }
        for parameter in &group.parameters {
            classify_live_reference(&mut summary, base, 7, owner.0, 2, parameter.0);
        }
        for default in &group.parameter_defaults {
            if let PublishedTypeParameterDefault::Ready(ty) = default {
                classify_live_reference(&mut summary, base, 7, owner.0, 1, ty.0);
            }
        }
        for alternative in &group.conflict_alternatives {
            for ty in &alternative.types {
                classify_live_reference(&mut summary, base, 7, owner.0, 1, ty.0);
            }
        }
    }
    for (class, terminal) in published.local_class_terminals() {
        classify_live_reference(&mut summary, base, 3, class.0, 3, class.0);
        let PublishedClassSnapshotTerminal::Ready(surface) = &terminal else {
            continue;
        };
        classify_live_reference(&mut summary, base, 3, class.0, 3, surface.class().0);
        for parameter in surface.type_params() {
            classify_live_reference(&mut summary, base, 3, class.0, 2, parameter.0);
        }
        for ty in [
            Some(surface.instance_template()),
            Some(surface.static_template()),
            surface.constructor_template(),
        ]
        .into_iter()
        .flatten()
        {
            classify_live_reference(&mut summary, base, 3, class.0, 1, ty.0);
        }
    }

    let namespace_terminals = pass
        .namespace_values
        .local_terminal_snapshot_parts_for_test()
        .expect("completed local namespace terminals snapshot");
    for row in namespace_terminals {
        classify_live_reference(&mut summary, base, 8, row.namespace.0, 8, row.namespace.0);
        if let FrozenNamespaceValueTerminalSnapshot::Ready { storage, ty } = row.terminal {
            classify_live_reference(&mut summary, base, 8, row.namespace.0, 9, storage.0);
            classify_live_reference(&mut summary, base, 8, row.namespace.0, 1, ty.0);
        }
    }

    for (&class, parameters) in pass.class_application_parameters.local_iter() {
        classify_live_reference(&mut summary, base, 3, class.0, 3, class.0);
        for parameter in parameters {
            let parameter = (*parameter).snapshot_parts();
            classify_live_reference(&mut summary, base, 3, class.0, 2, parameter.id.0);
            if let Some(constraint) = parameter.constraint {
                classify_live_reference(&mut summary, base, 3, class.0, 1, constraint.0);
            }
            if let ClassTypeParameterDefault::Ready(default) = parameter.default {
                classify_live_reference(&mut summary, base, 3, class.0, 1, default.0);
            }
        }
    }
    for (&class, metadata) in pass.class_new_metadata.local_iter() {
        classify_live_reference(&mut summary, base, 3, class.0, 3, class.0);
        classify_live_reference(
            &mut summary,
            base,
            3,
            class.0,
            3,
            metadata.ctor_declaring_class.0,
        );
    }
    for (&class, &parent) in pass.class_parents.local_iter() {
        classify_live_reference(&mut summary, base, 3, class.0, 3, parent.0);
    }
    for (&alias, &target) in pass.class_value_aliases.local_iter() {
        classify_live_reference(&mut summary, base, 9, alias.0, 9, target.0);
    }
    for (&storage, binding) in pass.class_value_bindings.local_iter() {
        classify_live_reference(&mut summary, base, 9, storage.0, 3, binding.class_id.0);
    }
    for (&alias, &target) in pass.standalone_namespace_value_aliases.local_iter() {
        classify_live_reference(&mut summary, base, 9, alias.0, 9, target.0);
    }
    for &symbol in pass.named_function_symbols.local_iter() {
        classify_live_reference(&mut summary, base, 5, symbol.0, 5, symbol.0);
    }
    for (&class, _) in pass.class_names.local_iter() {
        classify_live_reference(&mut summary, base, 3, class.0, 3, class.0);
    }
    if let Some(identities) = &pass.library_semantic_identities {
        for terminal in identities.terminals() {
            let LibraryIdentityTerminal::Ready(identity) = terminal else {
                continue;
            };
            for (domain, target) in std::iter::once((7, identity.group.0))
                .chain(std::iter::once((1, identity.template.0)))
                .chain(identity.parameters.iter().map(|parameter| (2, parameter.0)))
            {
                let Some(limit) = base_domain_limit(base, domain) else {
                    continue;
                };
                if usize::try_from(target).is_ok_and(|target| target >= limit) {
                    summary.base_to_delta += 1;
                }
            }
        }
    }
    summary
}

#[cfg(test)]
fn final_identity_witness(inputs: FinalIdentityInspection<'_>) -> OwnedBaseFinalIdentityWitness {
    let FinalIdentityInspection {
        base_store_len,
        base_value_storage_len,
        base_type_group_len,
        base_namespace_len,
        binder,
        published,
        interner,
        decl_types,
        next_type_param,
        next_class_id,
        actual_class_ids,
        references,
        base_row_clone_counts,
        local_rows_written,
    } = inputs;
    let mut named_alias_types = BTreeMap::new();
    let mut local_names = BTreeSet::new();
    let base_type_group_end =
        u32::try_from(base_type_group_len).expect("base type-group end fits u32");
    let base_namespace_end =
        u32::try_from(base_namespace_len).expect("base namespace end fits u32");
    let mut pending_scopes = vec![(binder.module, String::new())];
    let mut visited_namespaces = BTreeSet::new();
    while let Some((scope_id, prefix)) = pending_scopes.pop() {
        let Some(scope) = binder.graph.get(scope_id) else {
            continue;
        };
        for (name, symbol_id) in &scope.symbols {
            let qualified = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}.{name}")
            };
            local_names.insert(qualified.clone());
            let Some(symbol) = binder.symbols.get(*symbol_id) else {
                continue;
            };
            if let Some(group) = symbol.ty.filter(|group| group.0 >= base_type_group_end) {
                let alias = binder.type_groups.get(group).is_some_and(|declaration| {
                    declaration
                        .fragments
                        .iter()
                        .any(|fragment| fragment.kind == TypeFragmentKind::TypeAlias)
                });
                if alias {
                    if let Some(PublishedTypeGroupTerminal::Ready(ready)) =
                        published.groups().get(group)
                    {
                        if let PublishedTypeGroupSurface::Template(ty) = ready.surface {
                            named_alias_types.insert(qualified.clone(), ty);
                        }
                    }
                }
            }
            if let Some(namespace) = symbol
                .ns
                .filter(|namespace| namespace.0 >= base_namespace_end)
                .filter(|namespace| visited_namespaces.insert(*namespace))
            {
                if let Some(public_scope) = binder
                    .namespaces
                    .get(namespace)
                    .map(|namespace| namespace.public_scope)
                {
                    pending_scopes.push((public_scope, qualified));
                }
            }
        }
    }

    let eligible_base_shape = |ty: TypeId| {
        (ty.index() < base_store_len)
            .then(|| interner.store().tag(ty))
            .filter(|tag| {
                matches!(
                    tag,
                    TypeTag::Object | TypeTag::ClassInstance | TypeTag::Function
                )
            })
            .map(|tag| OwnedBaseReusedShapeWitness { type_id: ty, tag })
    };
    let direct_user_shape = named_alias_types
        .values()
        .copied()
        .chain(
            (base_value_storage_len..decl_types.len())
                .map(|index| ValueStorageId(u32::try_from(index).expect("storage id fits u32")))
                .filter_map(|storage| decl_types.get(storage)),
        )
        .find_map(eligible_base_shape);
    let reused_base_shape = direct_user_shape.or_else(|| {
        const TYPE_DOMAIN: u8 = 1;
        interner
            .local_type_reference_records_for_test()
            .into_iter()
            .find_map(|(owner_domain, target_domain, _, owner, target)| {
                (owner_domain == TYPE_DOMAIN
                    && target_domain == TYPE_DOMAIN
                    && usize::try_from(owner).ok()? >= base_store_len
                    && usize::try_from(target).ok()? < base_store_len)
                    .then_some(TypeId(target))
                    .and_then(eligible_base_shape)
            })
    });

    OwnedBaseFinalIdentityWitness {
        ends: OwnedBaseFinalIdentityEnds {
            store: interner.store().len(),
            declared_recipes: interner.store().snapshot_declared_recipes().count(),
            type_params: usize::try_from(next_type_param).expect("type parameter end fits usize"),
            classes: usize::try_from(next_class_id).expect("class end fits usize"),
            scopes: binder.graph.snapshot_len(),
            symbols: binder.symbols.len(),
            declarations: binder.declarations.len(),
            type_groups: binder.type_groups.len(),
            namespaces: binder.namespaces.len(),
            value_storages: decl_types.len(),
        },
        actual_ids: OwnedBaseActualIds {
            types: interner
                .store()
                .local_type_ids_for_test()
                .map(TypeId::index)
                .collect(),
            type_params: interner
                .store()
                .local_type_param_ids_for_test()
                .map(|parameter| {
                    usize::try_from(parameter.0).expect("type parameter id fits usize")
                })
                .collect(),
            classes: actual_class_ids
                .into_iter()
                .map(|class| usize::try_from(class.0).expect("class id fits usize"))
                .collect(),
            scopes: binder
                .graph
                .local_scopes()
                .map(|(scope, _)| usize::try_from(scope.0).expect("scope id fits usize"))
                .collect(),
            symbols: binder
                .symbols
                .local_symbols()
                .map(|(symbol, _)| usize::try_from(symbol.0).expect("symbol id fits usize"))
                .collect(),
            declarations: binder
                .declarations
                .local_declarations()
                .map(|declaration| {
                    usize::try_from(declaration.id.0).expect("declaration id fits usize")
                })
                .collect(),
            type_groups: binder
                .type_groups
                .local_groups()
                .map(|group| usize::try_from(group.id.0).expect("type group id fits usize"))
                .collect(),
            namespaces: binder
                .namespaces
                .local_namespaces()
                .map(|(namespace, _)| {
                    usize::try_from(namespace.0).expect("namespace id fits usize")
                })
                .collect(),
            value_storages: decl_types
                .local_slots()
                .map(|(storage, _)| {
                    usize::try_from(storage.0).expect("value storage id fits usize")
                })
                .collect(),
        },
        named_alias_types,
        local_names,
        references,
        reused_base_shape,
        base_row_clone_counts,
        local_rows_written,
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct InjectedLibrarySource<'source> {
    pub(crate) file_ordinal: LibraryFileOrdinal,
    pub(crate) name: &'source str,
    pub(crate) source: &'source str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InjectedProfileError {
    EmptyProfile,
    EmptyName {
        file_ordinal: LibraryFileOrdinal,
    },
    DuplicateName(String),
    DuplicateFileOrdinal(LibraryFileOrdinal),
    SourceKeyOverflow,
    Parse {
        file_ordinal: LibraryFileOrdinal,
        messages: Vec<String>,
    },
    Binder(String),
    Reservation(String),
    Reporting(LibraryEventLedgerError),
    ReplayIndex(ReplayIndexGenerationError),
    ReplayIndexAdmission(ReplayIndexAdmissionError),
    CanonicalProjection(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LibraryPhaseCounts {
    pub(crate) parse_units: usize,
    pub(crate) bind_units: usize,
    pub(crate) reserved_records: usize,
    pub(crate) filled_records: usize,
    pub(crate) publication_validations: usize,
    pub(crate) statement_check_units: usize,
}

#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LibraryPhaseTimings {
    pub(crate) parse: Duration,
    pub(crate) bind: Duration,
    pub(crate) reserve_fill: Duration,
    pub(crate) publication_validation: Duration,
    pub(crate) statement_check: Duration,
}

#[cfg(test)]
impl LibraryPhaseTimings {
    fn measured_total(&self) -> Duration {
        self.parse
            + self.bind
            + self.reserve_fill
            + self.publication_validation
            + self.statement_check
    }
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeProbe {
    pub(crate) identity: TypeGroupId,
    pub(crate) declaration_identities: Vec<(LibraryFileOrdinal, TypeGroupId)>,
    pub(crate) declaration_count: usize,
    pub(crate) member_names: Vec<String>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SignatureProbe {
    pub(crate) parameter_types: Vec<String>,
    pub(crate) return_type: String,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CallableMemberProbe {
    pub(crate) name: String,
    pub(crate) identity: ValueStorageId,
    pub(crate) source: ExactUnit,
    pub(crate) reservation_source: ExactUnit,
    pub(crate) source_start: u32,
    pub(crate) call_signature_count: usize,
    pub(crate) signatures: Vec<SignatureProbe>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValueProbe {
    pub(crate) identity: ValueStorageId,
    visible_type: Option<TypeId>,
    pub(crate) participant_identities: Vec<(LibraryFileOrdinal, ValueStorageId)>,
    pub(crate) declaration_count: usize,
    pub(crate) call_signature_count: usize,
    pub(crate) member_names: Vec<String>,
    pub(crate) callable_members: Vec<CallableMemberProbe>,
}

#[derive(Debug)]
pub(crate) struct InjectedProfileRun {
    pub(crate) phase_counts: LibraryPhaseCounts,
    #[cfg(test)]
    pub(crate) phase_timings: LibraryPhaseTimings,
    #[cfg(test)]
    pub(crate) reserved_file_ordinals: Vec<LibraryFileOrdinal>,
    #[cfg(test)]
    pub(crate) reporting_receipts: Vec<LibraryReportingReceipt>,
    pub(crate) library_records: Vec<(LibraryEventKey, CheckerRecord)>,
    pub(crate) evidence: CanonicalLibraryEvidence,
    #[cfg(test)]
    pub(crate) pass_source_units: Vec<ExactUnit>,
    #[cfg(test)]
    pub(crate) lexical_source_units: Vec<ExactUnit>,
    #[cfg(test)]
    global_types: BTreeMap<String, TypeProbe>,
    #[cfg(test)]
    module_types: BTreeMap<(LibraryFileOrdinal, String), TypeProbe>,
    #[cfg(test)]
    global_values: BTreeMap<String, ValueProbe>,
    #[cfg(test)]
    semantic_identities: LibrarySemanticIdentities,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CanonicalLibraryEvidence {
    pub(crate) diagnostics: Vec<u8>,
    pub(crate) incompletes: Vec<u8>,
    pub(crate) ledger: Vec<u8>,
}

impl InjectedProfileRun {
    #[cfg(test)]
    pub(crate) fn semantic_identities(&self) -> &LibrarySemanticIdentities {
        &self.semantic_identities
    }

    #[cfg(test)]
    pub(crate) fn global_type_probe(&self, name: &str) -> Option<&TypeProbe> {
        self.global_types.get(name)
    }

    #[cfg(test)]
    pub(crate) fn module_type_probe(
        &self,
        file_ordinal: LibraryFileOrdinal,
        name: &str,
    ) -> Option<&TypeProbe> {
        self.module_types.get(&(file_ordinal, name.to_owned()))
    }

    #[cfg(test)]
    pub(crate) fn global_value_probe(&self, name: &str) -> Option<&ValueProbe> {
        self.global_values.get(name)
    }
}

struct CanonicalInput<'source> {
    file_ordinal: LibraryFileOrdinal,
    source: &'source str,
    kind: SourceFileKind,
    source_key: ExactKey,
}

struct CanonicalLibraryFrontend<'source, 'ast> {
    canonical: Vec<CanonicalInput<'source>>,
    parsed: Vec<ParserReturn<'ast>>,
    binder: Binder,
    module_scopes: Vec<ScopeId>,
    semantic_scopes: Vec<ScopeId>,
    #[cfg(test)]
    parse_elapsed: Duration,
    #[cfg(test)]
    bind_elapsed: Duration,
}

#[cfg(test)]
thread_local! {
    static CANONICAL_FRONTEND_ENTRIES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CANONICAL_FRONTEND_PARSE_UNITS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CANONICAL_FRONTEND_BIND_BATCHES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CANONICAL_FRONTEND_BIND_UNITS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CANONICAL_FRONTEND_FULL_PRODUCTS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CANONICAL_FRONTEND_CHECKPOINT_PRODUCTS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CanonicalLibraryFrontendWorkForTest {
    pub(crate) entries: u64,
    pub(crate) parse_units: u64,
    pub(crate) bind_batches: u64,
    pub(crate) bind_units: u64,
    pub(crate) full_source_products_consumed: u64,
    pub(crate) checkpoint_products_consumed: u64,
}

#[cfg(test)]
fn canonical_library_frontend_work_for_test() -> CanonicalLibraryFrontendWorkForTest {
    CanonicalLibraryFrontendWorkForTest {
        entries: CANONICAL_FRONTEND_ENTRIES.get(),
        parse_units: CANONICAL_FRONTEND_PARSE_UNITS.get(),
        bind_batches: CANONICAL_FRONTEND_BIND_BATCHES.get(),
        bind_units: CANONICAL_FRONTEND_BIND_UNITS.get(),
        full_source_products_consumed: CANONICAL_FRONTEND_FULL_PRODUCTS.get(),
        checkpoint_products_consumed: CANONICAL_FRONTEND_CHECKPOINT_PRODUCTS.get(),
    }
}

#[cfg(test)]
pub(crate) struct CanonicalLibraryFrontendWorkScopeForTest(CanonicalLibraryFrontendWorkForTest);

#[cfg(test)]
impl CanonicalLibraryFrontendWorkScopeForTest {
    pub(crate) fn start() -> Self {
        Self(canonical_library_frontend_work_for_test())
    }

    pub(crate) fn finish(self) -> CanonicalLibraryFrontendWorkForTest {
        let end = canonical_library_frontend_work_for_test();
        CanonicalLibraryFrontendWorkForTest {
            entries: end.entries.saturating_sub(self.0.entries),
            parse_units: end.parse_units.saturating_sub(self.0.parse_units),
            bind_batches: end.bind_batches.saturating_sub(self.0.bind_batches),
            bind_units: end.bind_units.saturating_sub(self.0.bind_units),
            full_source_products_consumed: end
                .full_source_products_consumed
                .saturating_sub(self.0.full_source_products_consumed),
            checkpoint_products_consumed: end
                .checkpoint_products_consumed
                .saturating_sub(self.0.checkpoint_products_consumed),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct ParserExportClaim {
    file_ordinal: LibraryFileOrdinal,
    span: Span,
}

struct CanonicalBytes(Vec<u8>);

impl CanonicalBytes {
    fn domain(domain: &[u8]) -> Self {
        Self(Vec::from(domain))
    }

    fn byte(&mut self, value: u8) {
        self.0.push(value);
    }

    #[cfg(test)]
    fn bool(&mut self, value: bool) {
        self.byte(u8::from(value));
    }

    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    fn usize(&mut self, value: usize) -> Result<(), InjectedProfileError> {
        self.u64(u64::try_from(value).map_err(|_| {
            InjectedProfileError::CanonicalProjection("usize does not fit u64".to_owned())
        })?);
        Ok(())
    }

    fn bytes(&mut self, value: &[u8]) -> Result<(), InjectedProfileError> {
        self.usize(value.len())?;
        self.0.extend_from_slice(value);
        Ok(())
    }

    fn string(&mut self, value: &str) -> Result<(), InjectedProfileError> {
        self.bytes(value.as_bytes())
    }

    #[cfg(test)]
    fn type_id(&mut self, value: TypeId) {
        self.u32(value.0);
    }

    #[cfg(test)]
    fn optional_type_id(&mut self, value: Option<TypeId>) {
        self.bool(value.is_some());
        if let Some(value) = value {
            self.type_id(value);
        }
    }

    #[cfg(test)]
    fn type_ids(&mut self, values: &[TypeId]) -> Result<(), InjectedProfileError> {
        self.usize(values.len())?;
        for value in values {
            self.type_id(*value);
        }
        Ok(())
    }

    fn finish(self) -> Vec<u8> {
        self.0
    }
}

#[cfg(test)]
fn intrinsic_code(kind: IntrinsicKind) -> u8 {
    match kind {
        IntrinsicKind::Error => 0,
        IntrinsicKind::Any => 1,
        IntrinsicKind::Unknown => 2,
        IntrinsicKind::Never => 3,
        IntrinsicKind::Void => 4,
        IntrinsicKind::Null => 5,
        IntrinsicKind::Undefined => 6,
        IntrinsicKind::Boolean => 7,
        IntrinsicKind::Number => 8,
        IntrinsicKind::String => 9,
        IntrinsicKind::Uppercase => 10,
        IntrinsicKind::Lowercase => 11,
        IntrinsicKind::Capitalize => 12,
        IntrinsicKind::Uncapitalize => 13,
        IntrinsicKind::ThisType => 14,
        IntrinsicKind::OmitThisParameter => 15,
        IntrinsicKind::Object => 16,
    }
}

#[cfg(test)]
fn visibility_code(visibility: Visibility) -> u8 {
    match visibility {
        Visibility::Public => 0,
        Visibility::Private => 1,
        Visibility::Protected => 2,
    }
}

#[cfg(test)]
fn modifier_code(modifier: ModifierOp) -> u8 {
    match modifier {
        ModifierOp::Keep => 0,
        ModifierOp::Add => 1,
        ModifierOp::Remove => 2,
    }
}

#[cfg(test)]
fn encode_store_row(
    bytes: &mut CanonicalBytes,
    store: &Store,
    id: TypeId,
) -> Result<(), InjectedProfileError> {
    let tag = store.tag(id);
    bytes.type_id(id);
    bytes.byte(tag.discriminant());
    bytes.u32(store.flags(id).0);
    match tag {
        TypeTag::Intrinsic => bytes.byte(intrinsic_code(store.intrinsic_kind(id).ok_or_else(
            || InjectedProfileError::CanonicalProjection("missing intrinsic payload".to_owned()),
        )?)),
        TypeTag::Literal => match store.literal_value(id).ok_or_else(|| {
            InjectedProfileError::CanonicalProjection("missing literal payload".to_owned())
        })? {
            LiteralValue::Number(value) => {
                bytes.byte(0);
                bytes.u64(value.to_bits());
            }
            LiteralValue::String(value) => {
                bytes.byte(1);
                bytes.string(value)?;
            }
            LiteralValue::Boolean(value) => {
                bytes.byte(2);
                bytes.bool(*value);
            }
        },
        TypeTag::Object => {
            let object = store.object_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing object payload".to_owned())
            })?;
            bytes.usize(object.properties.len())?;
            for property in &object.properties {
                bytes.string(&property.name)?;
                bytes.type_id(property.ty);
                bytes.optional_type_id(property.write_ty);
                bytes.bool(property.optional);
                bytes.byte(visibility_code(property.visibility));
                bytes.bool(property.declaring_class.is_some());
                if let Some(class) = property.declaring_class {
                    bytes.u32(class.0);
                }
                bytes.bool(property.readonly);
                bytes.bool(property.is_accessor);
            }
            bytes.optional_type_id(object.string_index);
            bytes.optional_type_id(object.number_index);
            bytes.type_ids(&object.call_signatures)?;
            bytes.type_ids(&object.construct_signatures)?;
        }
        TypeTag::Union => bytes.type_ids(store.union_members(id).ok_or_else(|| {
            InjectedProfileError::CanonicalProjection("missing union payload".to_owned())
        })?)?,
        TypeTag::Intersection => {
            bytes.type_ids(store.intersection_members(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing intersection payload".to_owned())
            })?)?
        }
        TypeTag::Function => {
            let function = store.function_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing function payload".to_owned())
            })?;
            bytes.usize(function.type_params.len())?;
            for parameter in &function.type_params {
                bytes.u32(parameter.id.0);
                bytes.optional_type_id(parameter.constraint);
                bytes.optional_type_id(parameter.default);
            }
            bytes.optional_type_id(function.receiver);
            bytes.usize(function.params.len())?;
            for parameter in &function.params {
                bytes.string(&parameter.name)?;
                bytes.type_id(parameter.ty);
                bytes.bool(parameter.optional);
                bytes.bool(parameter.has_default);
                bytes.bool(parameter.rest);
            }
            bytes.type_id(function.ret);
        }
        TypeTag::TypeParam => {
            let parameter = store.type_param(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing type parameter".to_owned())
            })?;
            bytes.u32(parameter.id.0);
            bytes.string(&parameter.name)?;
            bytes.optional_type_id(store.type_param_constraint(parameter.id));
            bytes.bool(store.type_param_metadata_is_frozen(parameter.id));
        }
        TypeTag::Array => bytes.type_id(
            store
                .array_type(id)
                .ok_or_else(|| {
                    InjectedProfileError::CanonicalProjection("missing array payload".to_owned())
                })?
                .element,
        ),
        TypeTag::Tuple => {
            let tuple = store.tuple_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing tuple payload".to_owned())
            })?;
            bytes.type_ids(&tuple.elements)?;
            bytes.bool(tuple.rest.is_some());
            if let Some(rest) = tuple.rest {
                bytes.usize(rest.position)?;
                bytes.type_id(rest.ty);
            }
        }
        TypeTag::Readonly => bytes.type_id(store.readonly_operand(id).ok_or_else(|| {
            InjectedProfileError::CanonicalProjection("missing readonly operand".to_owned())
        })?),
        TypeTag::Conditional => {
            let conditional = store.conditional_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing conditional".to_owned())
            })?;
            bytes.type_id(conditional.check);
            bytes.type_id(conditional.extends_ty);
            bytes.type_id(conditional.true_branch);
            bytes.type_id(conditional.false_branch);
            bytes.u32(conditional.infer_count);
            bytes.bool(conditional.distributive);
            bytes.bool(conditional.poisoned);
        }
        TypeTag::Instantiation => {
            let instantiation = store.instantiation_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing instantiation".to_owned())
            })?;
            bytes.type_id(instantiation.base);
            bytes.usize(instantiation.args.len())?;
            for (parameter, argument) in &instantiation.args {
                bytes.u32(parameter.0);
                bytes.type_id(*argument);
            }
        }
        TypeTag::Infer => bytes.u32(store.infer_index(id).ok_or_else(|| {
            InjectedProfileError::CanonicalProjection("missing infer index".to_owned())
        })?),
        TypeTag::Mapped => {
            let mapped = store.mapped_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing mapped payload".to_owned())
            })?;
            bytes.bool(mapped.homomorphic);
            bytes.type_id(mapped.key_source);
            bytes.type_id(mapped.value_template);
            bytes.optional_type_id(mapped.modifiers_source);
            bytes.byte(modifier_code(mapped.optional_modifier));
            bytes.byte(modifier_code(mapped.readonly_modifier));
        }
        TypeTag::MappedValue => {}
        TypeTag::Template => {
            let template = store.template_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing template payload".to_owned())
            })?;
            bytes.usize(template.texts.len())?;
            for text in &template.texts {
                bytes.string(text)?;
            }
            bytes.type_ids(&template.holes)?;
        }
        TypeTag::Keyof => bytes.type_id(store.keyof_operand(id).ok_or_else(|| {
            InjectedProfileError::CanonicalProjection("missing keyof operand".to_owned())
        })?),
        TypeTag::ClassInstance => {
            let instance = store.class_instance_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing class instance".to_owned())
            })?;
            bytes.u32(instance.class.0);
            bytes.type_ids(&instance.args)?;
        }
        TypeTag::DeferredIndexedAccess => {
            let access = store.deferred_indexed_access_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection(
                    "missing deferred indexed access".to_owned(),
                )
            })?;
            bytes.type_id(access.object);
            bytes.type_id(access.index);
        }
        TypeTag::Declared => {
            let declared = store.declared_type(id).ok_or_else(|| {
                InjectedProfileError::CanonicalProjection("missing declared payload".to_owned())
            })?;
            bytes.u32(declared.recipe.0);
            bytes.usize(declared.mapper.len())?;
            for (parameter, value) in &declared.mapper {
                bytes.u32(parameter.0);
                bytes.type_id(*value);
            }
        }
    }
    bytes.bool(store.template_name(id).is_some());
    if let Some(name) = store.template_name(id) {
        bytes.string(name)?;
    }
    Ok(())
}

#[cfg(test)]
fn encode_declared_recipe_row(
    bytes: &mut CanonicalBytes,
    store: &Store,
    id: crate::types::repr::DeclaredRecipeId,
) -> Result<(), InjectedProfileError> {
    use crate::types::repr::DeclaredRecipeNode;
    let recipe = store.declared_recipe(id).ok_or_else(|| {
        InjectedProfileError::CanonicalProjection("missing declared recipe".to_owned())
    })?;
    match &recipe.node {
        DeclaredRecipeNode::Type(ty) => {
            bytes.byte(0);
            bytes.type_id(*ty);
        }
        DeclaredRecipeNode::Array(element) => {
            bytes.byte(1);
            bytes.u32(element.0);
        }
        DeclaredRecipeNode::Tuple { elements, rest } => {
            bytes.byte(2);
            bytes.usize(elements.len())?;
            for element in elements {
                bytes.u32(element.0);
            }
            bytes.bool(rest.is_some());
            if let Some((position, rest)) = rest {
                bytes.usize(*position)?;
                bytes.u32(rest.0);
            }
        }
        DeclaredRecipeNode::Readonly(operand) => {
            bytes.byte(3);
            bytes.u32(operand.0);
        }
        DeclaredRecipeNode::Application {
            template,
            parameters,
            arguments,
        } => {
            bytes.byte(4);
            bytes.type_id(*template);
            bytes.usize(parameters.len())?;
            for parameter in parameters {
                bytes.u32(parameter.0);
            }
            bytes.usize(arguments.len())?;
            for argument in arguments {
                bytes.u32(argument.0);
            }
        }
    }
    bytes.usize(recipe.free_params.len())?;
    for parameter in &recipe.free_params {
        bytes.u32(parameter.0);
    }
    Ok(())
}

fn canonical_input_for_record(
    canonical: &[CanonicalInput<'_>],
    file_ordinal: LibraryFileOrdinal,
) -> Result<usize, InjectedProfileError> {
    canonical
        .iter()
        .position(|input| input.file_ordinal == file_ordinal)
        .ok_or_else(|| {
            InjectedProfileError::CanonicalProjection(
                "library record has no canonical source".to_owned(),
            )
        })
}

fn encode_library_key(
    bytes: &mut CanonicalBytes,
    key: LibraryEventKey,
) -> Result<(), InjectedProfileError> {
    bytes.usize(key.file_ordinal.index())?;
    bytes.u32(key.source_start);
    bytes.usize(key.event_ordinal)?;
    bytes.usize(key.record_ordinal)?;
    Ok(())
}

fn canonical_record_bytes(
    source: &str,
    record: &CheckerRecord,
) -> Result<Vec<u8>, InjectedProfileError> {
    let mut bytes = CanonicalBytes::domain(b"typokat-wu0d-library-record-v1");
    match record {
        CheckerRecord::Diagnostic(diagnostic) => {
            bytes.byte(0);
            bytes.u32(diagnostic.span.start);
            bytes.u32(diagnostic.span.end);
            let mut rendered = Vec::new();
            render_to_writer_with_format(
                &mut rendered,
                "library",
                source,
                std::slice::from_ref(diagnostic),
                DiagnosticFormat::Compact,
            )
            .map_err(|error| InjectedProfileError::CanonicalProjection(error.to_string()))?;
            bytes.bytes(&rendered)?;
        }
        CheckerRecord::Incomplete(incomplete) => {
            bytes.byte(1);
            bytes.string(&incomplete.id)?;
            bytes.u32(incomplete.span.start);
            bytes.u32(incomplete.span.end);
            bytes.string(&incomplete.context)?;
        }
    }
    Ok(bytes.finish())
}

fn canonical_library_evidence(
    canonical: &[CanonicalInput<'_>],
    records: &[(LibraryEventKey, CheckerRecord)],
) -> Result<CanonicalLibraryEvidence, InjectedProfileError> {
    let mut diagnostics = CanonicalBytes::domain(b"typokat-wu0d-diagnostics-v1");
    let mut incompletes = CanonicalBytes::domain(b"typokat-wu0d-incomplete-v1");
    let mut ledger = CanonicalBytes::domain(b"typokat-wu0d-library-ledger-v1");
    let diagnostic_count = records
        .iter()
        .filter(|(_, record)| matches!(record, CheckerRecord::Diagnostic(_)))
        .count();
    diagnostics.usize(diagnostic_count)?;
    incompletes.usize(records.len() - diagnostic_count)?;
    ledger.usize(records.len())?;
    for (key, record) in records {
        let input = canonical_input_for_record(canonical, key.file_ordinal)?;
        let record_bytes = canonical_record_bytes(canonical[input].source, record)?;
        encode_library_key(&mut ledger, *key)?;
        ledger.bytes(&record_bytes)?;
        let component = match record {
            CheckerRecord::Diagnostic(_) => &mut diagnostics,
            CheckerRecord::Incomplete(_) => &mut incompletes,
        };
        encode_library_key(component, *key)?;
        component.bytes(&record_bytes)?;
    }
    Ok(CanonicalLibraryEvidence {
        diagnostics: diagnostics.finish(),
        incompletes: incompletes.finish(),
        ledger: ledger.finish(),
    })
}

struct ReplayTerminalValidationInputs<'a> {
    binder: &'a Binder,
    published: &'a PublishedTypeEnvironment,
    decl_types: &'a DeclTypes,
    namespace_terminals: &'a [FrozenNamespaceValueTerminalSnapshotRow],
    runtime: &'a FrozenCheckerRuntimeSnapshotParts,
    semantic_identities: Option<&'a LibrarySemanticIdentities>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TerminalClassDependencyValidationWork {
    owner_root_summaries: u64,
    semantic_node_visits: u64,
    semantic_edge_probes: u64,
    class_edge_probes: u64,
    class_summary_items: u64,
    owner_expression_visits: u64,
}

fn intern_terminal_semantic_node(
    nodes: &mut rustc_hash::FxHashMap<(u8, u32), usize>,
    semantic_edges: &mut Vec<Vec<usize>>,
    reverse_edges: &mut Vec<Vec<usize>>,
    class_edges: &mut Vec<Vec<ClassId>>,
    key: (u8, u32),
) -> usize {
    if let Some(node) = nodes.get(&key) {
        return *node;
    }
    let node = semantic_edges.len();
    nodes.insert(key, node);
    semantic_edges.push(Vec::new());
    reverse_edges.push(Vec::new());
    class_edges.push(Vec::new());
    node
}

enum TerminalOwnerExpression {
    Owner(ReplayOwner),
    Union(Vec<usize>),
}

/// Flattens every owner expression in one ascending pass. A `Union` is always
/// minted after the inputs it merges, so the vector is already topologically
/// ordered and each expression is summarized exactly once — memoizing only at
/// query roots re-walked shared pass-through chains once per root.
fn terminal_expression_owners(
    expressions: &[TerminalOwnerExpression],
    #[cfg(test)] work: &mut Option<&mut TerminalClassDependencyValidationWork>,
) -> Vec<Vec<ReplayOwner>> {
    let mut owners = Vec::<Vec<ReplayOwner>>::with_capacity(expressions.len());
    for (expression, node) in expressions.iter().enumerate() {
        #[cfg(test)]
        record_terminal_owner_expression_visits(work, 1);
        match node {
            TerminalOwnerExpression::Owner(owner) => owners.push(vec![*owner]),
            TerminalOwnerExpression::Union(inputs) => {
                let mut merged = Vec::new();
                for input in inputs.iter().copied() {
                    // Ascending order is the precondition of this pass; a forward
                    // reference would silently truncate the owner closure.
                    assert!(
                        input < expression,
                        "owner expression {expression} merges a later input {input}"
                    );
                    #[cfg(test)]
                    record_terminal_owner_expression_visits(work, owners[input].len());
                    merged.extend_from_slice(&owners[input]);
                }
                merged.sort_unstable();
                merged.dedup();
                owners.push(merged);
            }
        }
    }
    owners
}

#[cfg(test)]
fn record_terminal_semantic_nodes(
    work: &mut Option<&mut TerminalClassDependencyValidationWork>,
    count: usize,
) {
    if let Some(work) = work.as_deref_mut() {
        work.semantic_node_visits = work
            .semantic_node_visits
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
    }
}

#[cfg(test)]
fn record_terminal_semantic_edges(
    work: &mut Option<&mut TerminalClassDependencyValidationWork>,
    count: usize,
) {
    if let Some(work) = work.as_deref_mut() {
        work.semantic_edge_probes = work
            .semantic_edge_probes
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
    }
}

#[cfg(test)]
fn record_terminal_class_edges(
    work: &mut Option<&mut TerminalClassDependencyValidationWork>,
    count: usize,
) {
    if let Some(work) = work.as_deref_mut() {
        work.class_edge_probes = work
            .class_edge_probes
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
    }
}

#[cfg(test)]
fn record_terminal_class_summary_items(
    work: &mut Option<&mut TerminalClassDependencyValidationWork>,
    count: usize,
) {
    if let Some(work) = work.as_deref_mut() {
        work.class_summary_items = work
            .class_summary_items
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
    }
}

#[cfg(test)]
fn record_terminal_owner_expression_visits(
    work: &mut Option<&mut TerminalClassDependencyValidationWork>,
    count: usize,
) {
    if let Some(work) = work.as_deref_mut() {
        work.owner_expression_visits = work
            .owner_expression_visits
            .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
    }
}

fn require_terminal_class_dependency_closure(
    trace: &ReplayDependencyTrace,
    sparse_semantic_edges: &BTreeMap<(u8, u32), Vec<(u8, u32)>>,
    sparse_class_edges: &BTreeMap<(u8, u32), Vec<ClassId>>,
    direct: BTreeMap<ReplayOwner, Vec<TypeId>>,
    #[cfg(test)] mut work: Option<&mut TerminalClassDependencyValidationWork>,
) {
    const TYPE_DOMAIN: u8 = 1;

    let mut nodes = rustc_hash::FxHashMap::default();
    let mut semantic_edges = Vec::<Vec<usize>>::new();
    let mut reverse_edges = Vec::<Vec<usize>>::new();
    let mut class_edges = Vec::<Vec<ClassId>>::new();
    for (source_key, targets) in sparse_semantic_edges {
        let source = intern_terminal_semantic_node(
            &mut nodes,
            &mut semantic_edges,
            &mut reverse_edges,
            &mut class_edges,
            *source_key,
        );
        for target_key in targets {
            let target = intern_terminal_semantic_node(
                &mut nodes,
                &mut semantic_edges,
                &mut reverse_edges,
                &mut class_edges,
                *target_key,
            );
            semantic_edges[source].push(target);
            reverse_edges[target].push(source);
        }
    }
    for (source_key, classes) in sparse_class_edges {
        let source = intern_terminal_semantic_node(
            &mut nodes,
            &mut semantic_edges,
            &mut reverse_edges,
            &mut class_edges,
            *source_key,
        );
        #[cfg(test)]
        {
            record_terminal_class_edges(&mut work, classes.len());
            record_terminal_class_summary_items(&mut work, classes.len());
        }
        class_edges[source].extend(classes.iter().copied());
    }
    for roots in direct.values() {
        for root in roots {
            intern_terminal_semantic_node(
                &mut nodes,
                &mut semantic_edges,
                &mut reverse_edges,
                &mut class_edges,
                (TYPE_DOMAIN, root.0),
            );
        }
    }
    for classes in &mut class_edges {
        classes.sort_unstable();
        classes.dedup();
    }

    // Condense cycles before propagating terminal classes across shared tails.
    let node_count = semantic_edges.len();
    let mut finish_order = Vec::with_capacity(node_count);
    let mut seen = vec![false; node_count];
    for start in 0..node_count {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        #[cfg(test)]
        record_terminal_semantic_nodes(&mut work, 1);
        let mut pending = vec![(start, 0_usize)];
        while let Some((node, next_edge)) = pending.last_mut() {
            if *next_edge == semantic_edges[*node].len() {
                finish_order.push(*node);
                pending.pop();
                continue;
            }
            let target = semantic_edges[*node][*next_edge];
            *next_edge += 1;
            #[cfg(test)]
            record_terminal_semantic_edges(&mut work, 1);
            if !seen[target] {
                seen[target] = true;
                #[cfg(test)]
                record_terminal_semantic_nodes(&mut work, 1);
                pending.push((target, 0));
            }
        }
    }

    let mut component_of = vec![usize::MAX; node_count];
    let mut component_count = 0;
    while let Some(start) = finish_order.pop() {
        if component_of[start] != usize::MAX {
            continue;
        }
        component_of[start] = component_count;
        let mut pending = vec![start];
        while let Some(node) = pending.pop() {
            #[cfg(test)]
            record_terminal_semantic_nodes(&mut work, 1);
            for source in &reverse_edges[node] {
                #[cfg(test)]
                record_terminal_semantic_edges(&mut work, 1);
                if component_of[*source] == usize::MAX {
                    component_of[*source] = component_count;
                    pending.push(*source);
                }
            }
        }
        component_count += 1;
    }

    let mut component_dependencies = vec![Vec::<usize>::new(); component_count];
    let mut component_dependents = vec![Vec::<usize>::new(); component_count];
    let mut component_classes = vec![Vec::<ClassId>::new(); component_count];
    for node in 0..node_count {
        #[cfg(test)]
        record_terminal_semantic_nodes(&mut work, 1);
        let component = component_of[node];
        #[cfg(test)]
        {
            record_terminal_class_edges(&mut work, class_edges[node].len());
            record_terminal_class_summary_items(&mut work, class_edges[node].len());
        }
        component_classes[component].extend_from_slice(&class_edges[node]);
        for target in &semantic_edges[node] {
            #[cfg(test)]
            record_terminal_semantic_edges(&mut work, 1);
            let dependency = component_of[*target];
            if component != dependency {
                component_dependencies[component].push(dependency);
            }
        }
    }
    for classes in &mut component_classes {
        classes.sort_unstable();
        classes.dedup();
    }
    for dependencies in &mut component_dependencies {
        dependencies.sort_unstable();
        dependencies.dedup();
    }
    for (component, dependencies) in component_dependencies.iter().enumerate() {
        for dependency in dependencies {
            component_dependents[*dependency].push(component);
        }
    }

    let mut expressions = direct
        .keys()
        .copied()
        .map(TerminalOwnerExpression::Owner)
        .collect::<Vec<_>>();
    let owner_expressions = direct
        .keys()
        .copied()
        .enumerate()
        .map(|(expression, owner)| (owner, expression))
        .collect::<BTreeMap<_, _>>();
    let mut component_owner_inputs = vec![Vec::<usize>::new(); component_count];
    for (owner, roots) in &direct {
        #[cfg(test)]
        if let Some(work) = work.as_deref_mut() {
            work.owner_root_summaries = work
                .owner_root_summaries
                .saturating_add(u64::try_from(roots.len()).unwrap_or(u64::MAX));
        }
        let expression = owner_expressions[owner];
        for root in roots {
            let node = nodes[&(TYPE_DOMAIN, root.0)];
            component_owner_inputs[component_of[node]].push(expression);
        }
    }

    // Owner expressions stay persistent across pass-through chains. They are
    // flattened only where terminal classes make the owner × class output real.
    let mut unresolved_dependents = component_dependents
        .iter()
        .map(Vec::len)
        .collect::<Vec<_>>();
    let mut ready = unresolved_dependents
        .iter()
        .enumerate()
        .filter_map(|(component, count)| (*count == 0).then_some(component))
        .collect::<Vec<_>>();
    let mut processed_components = 0;
    let mut class_owner_inputs = BTreeMap::<ClassId, Vec<usize>>::new();
    while let Some(component) = ready.pop() {
        processed_components += 1;
        #[cfg(test)]
        record_terminal_semantic_nodes(&mut work, 1);

        component_owner_inputs[component].sort_unstable();
        component_owner_inputs[component].dedup();
        let owner_expression = match component_owner_inputs[component].as_slice() {
            [] => None,
            [expression] => Some(*expression),
            inputs => {
                let expression = expressions.len();
                expressions.push(TerminalOwnerExpression::Union(inputs.to_vec()));
                Some(expression)
            }
        };

        if let Some(expression) = owner_expression {
            #[cfg(test)]
            {
                record_terminal_class_edges(&mut work, component_classes[component].len());
                record_terminal_class_summary_items(&mut work, component_classes[component].len());
            }
            for class in &component_classes[component] {
                class_owner_inputs
                    .entry(*class)
                    .or_default()
                    .push(expression);
            }
            for dependency in component_dependencies[component].iter().copied() {
                component_owner_inputs[dependency].push(expression);
            }
        }

        for dependency in component_dependencies[component].iter().copied() {
            #[cfg(test)]
            record_terminal_semantic_edges(&mut work, 1);
            unresolved_dependents[dependency] -= 1;
            if unresolved_dependents[dependency] == 0 {
                ready.push(dependency);
            }
        }
    }
    // An undrained condensation would emit no classes for the stranded components,
    // i.e. silently admit unauthenticated replay dependencies.
    assert_eq!(
        processed_components, component_count,
        "the terminal condensation is acyclic and must drain completely"
    );

    let mut class_expressions = Vec::with_capacity(class_owner_inputs.len());
    for (class, mut inputs) in class_owner_inputs {
        inputs.sort_unstable();
        inputs.dedup();
        let expression = match inputs.as_slice() {
            [expression] => *expression,
            inputs => {
                let expression = expressions.len();
                expressions.push(TerminalOwnerExpression::Union(inputs.to_vec()));
                expression
            }
        };
        class_expressions.push((class, expression));
    }
    let expression_owners = terminal_expression_owners(
        &expressions,
        #[cfg(test)]
        &mut work,
    );
    for (class, expression) in class_expressions {
        let owners = &expression_owners[expression];
        #[cfg(test)]
        {
            record_terminal_class_edges(&mut work, owners.len());
            record_terminal_class_summary_items(&mut work, owners.len());
        }
        for owner in owners {
            trace.require_dependency(*owner, ReplayOwner::Class(class));
        }
    }
}

fn validate_terminal_class_dependencies(
    trace: &ReplayDependencyTrace,
    interner: &Interner,
    inputs: ReplayTerminalValidationInputs<'_>,
) -> Result<(), InjectedProfileError> {
    let ReplayTerminalValidationInputs {
        binder,
        published,
        decl_types,
        namespace_terminals,
        runtime,
        semantic_identities,
    } = inputs;
    const TYPE_DOMAIN: u8 = 1;
    const CLASS_DOMAIN: u8 = 3;
    const DECLARED_RECIPE_DOMAIN: u8 = 10;

    let references = interner
        .typed_reference_records_for_replay_generation()
        .map_err(|error| InjectedProfileError::CanonicalProjection(error.to_string()))?;
    let mut semantic_edges: BTreeMap<(u8, u32), Vec<(u8, u32)>> = BTreeMap::new();
    let mut class_edges: BTreeMap<(u8, u32), Vec<ClassId>> = BTreeMap::new();
    for (owner_domain, target_domain, _, owner, target) in references {
        if !matches!(owner_domain, TYPE_DOMAIN | DECLARED_RECIPE_DOMAIN) {
            continue;
        }
        match target_domain {
            TYPE_DOMAIN | DECLARED_RECIPE_DOMAIN => semantic_edges
                .entry((owner_domain, owner))
                .or_default()
                .push((target_domain, target)),
            CLASS_DOMAIN => class_edges
                .entry((owner_domain, owner))
                .or_default()
                .push(ClassId(target)),
            _ => {}
        }
    }

    let mut direct = BTreeMap::<ReplayOwner, Vec<TypeId>>::new();
    for index in 0..published.groups().len() {
        let group =
            TypeGroupId(u32::try_from(index).map_err(|_| InjectedProfileError::SourceKeyOverflow)?);
        let Some(PublishedTypeGroupTerminal::Ready(ready)) = published.groups().get(group) else {
            continue;
        };
        let owner = ReplayOwner::TypeGroup(group);
        match ready.surface {
            PublishedTypeGroupSurface::Template(ty) => direct.entry(owner).or_default().push(ty),
            PublishedTypeGroupSurface::Class(class) => {
                trace.require_dependency(owner, ReplayOwner::Class(class));
            }
        }
        direct
            .entry(owner)
            .or_default()
            .extend(
                ready
                    .parameter_defaults
                    .iter()
                    .filter_map(|default| match default {
                        PublishedTypeParameterDefault::Ready(ty) => Some(*ty),
                        PublishedTypeParameterDefault::Absent
                        | PublishedTypeParameterDefault::Unsupported => None,
                    }),
            );
        direct.entry(owner).or_default().extend(
            ready
                .conflict_alternatives
                .iter()
                .flat_map(|alternative| alternative.types.iter().copied()),
        );
    }
    for (index, ty) in decl_types.snapshot_slots().into_iter().enumerate() {
        let Some(ty) = ty else { continue };
        let storage = ValueStorageId(
            u32::try_from(index).map_err(|_| InjectedProfileError::SourceKeyOverflow)?,
        );
        direct
            .entry(ReplayOwner::Value(storage))
            .or_default()
            .push(ty);
    }
    for row in namespace_terminals {
        let FrozenNamespaceValueTerminalSnapshot::Ready { storage, ty } = row.terminal else {
            continue;
        };
        let owner = ReplayOwner::Namespace(row.namespace);
        trace.require_dependency(owner, ReplayOwner::Value(storage));
        direct.entry(owner).or_default().push(ty);
    }
    let terminals = published.classes().canonical_terminals().ok_or_else(|| {
        InjectedProfileError::CanonicalProjection(
            "replay generation saw a non-terminal class registry".to_owned(),
        )
    })?;
    for (class, terminal) in &terminals {
        let CanonicalPublishedClassTerminal::Ready(surface) = terminal else {
            continue;
        };
        let types = direct.entry(ReplayOwner::Class(*class)).or_default();
        types.push(surface.instance_template());
        types.push(surface.static_template());
        types.extend(surface.constructor_template());
    }

    for (class, parameters) in &runtime.class_application_parameters {
        let types = direct.entry(ReplayOwner::Class(*class)).or_default();
        for parameter in parameters {
            types.extend(parameter.constraint);
            if let ClassTypeParameterDefault::Ready(default) = parameter.default {
                types.push(default);
            }
        }
    }
    for (class, metadata) in &runtime.class_new_metadata {
        trace.require_dependency(
            ReplayOwner::Class(*class),
            ReplayOwner::Class(metadata.ctor_declaring_class),
        );
    }
    for (class, parent) in &runtime.class_parents {
        trace.require_dependency(ReplayOwner::Class(*class), ReplayOwner::Class(*parent));
    }
    for (alias, target) in &runtime.class_value_aliases {
        trace.require_dependency(ReplayOwner::Value(*alias), ReplayOwner::Value(*target));
    }
    for (storage, binding) in &runtime.class_value_bindings {
        trace.require_dependency(
            ReplayOwner::Value(*storage),
            ReplayOwner::Class(binding.class_id),
        );
    }
    for (alias, target) in &runtime.standalone_namespace_value_aliases {
        trace.require_dependency(ReplayOwner::Value(*alias), ReplayOwner::Value(*target));
    }
    let class_ids = terminals
        .iter()
        .map(|(class, _)| *class)
        .collect::<BTreeSet<_>>();
    if runtime
        .class_names
        .iter()
        .any(|(class, _)| !class_ids.contains(class))
    {
        return Err(InjectedProfileError::CanonicalProjection(
            "replay runtime class name has no published class owner".to_owned(),
        ));
    }
    for symbol in &runtime.named_function_symbols {
        let binding = binder.symbols.get(*symbol).ok_or_else(|| {
            InjectedProfileError::CanonicalProjection(
                "replay runtime named function symbol is missing".to_owned(),
            )
        })?;
        let Some(canonical) = binding.function_values.first().copied() else {
            return Err(InjectedProfileError::CanonicalProjection(
                "replay runtime named function has no value owner".to_owned(),
            ));
        };
        for participant in &binding.function_values {
            trace.require_dependency(
                ReplayOwner::Value(canonical),
                ReplayOwner::Value(*participant),
            );
        }
    }
    if let Some(identities) = semantic_identities {
        let recomputed = LibrarySemanticIdentities::select(binder, published, interner.store());
        if &recomputed != identities {
            return Err(InjectedProfileError::CanonicalProjection(
                "replay semantic identities differ from exact recomputation".to_owned(),
            ));
        }
        for terminal in identities.terminals() {
            let LibraryIdentityTerminal::Ready(identity) = terminal else {
                return Err(InjectedProfileError::CanonicalProjection(
                    "replay semantic identity is unavailable".to_owned(),
                ));
            };
            direct
                .entry(ReplayOwner::TypeGroup(identity.group))
                .or_default()
                .push(identity.template);
        }
    }

    require_terminal_class_dependency_closure(
        trace,
        &semantic_edges,
        &class_edges,
        direct,
        #[cfg(test)]
        None,
    );
    Ok(())
}

fn validate_root_census(
    candidates: &BTreeMap<String, crate::binder::declaration::SourceGlobalBindingCandidate>,
    rows: &[RootNameRow],
    explicit_global_this: bool,
) -> Result<(), ReplayIndexGenerationError> {
    let mut by_name = BTreeMap::new();
    for row in rows {
        if by_name.insert(row.name.as_str(), row).is_some() {
            return Err(ReplayIndexGenerationError::DuplicateRootName(
                row.name.clone(),
            ));
        }
    }
    for (name, candidate) in candidates {
        let Some(row) = by_name.get(name.as_str()).copied() else {
            return Err(ReplayIndexGenerationError::InvalidRootSlot(name.clone()));
        };
        for (slot, present) in [
            (SourceBindingSlot::Value, row.value.is_some()),
            (SourceBindingSlot::Type, row.ty.is_some()),
            (SourceBindingSlot::Namespace, row.namespace.is_some()),
        ] {
            if candidate.slots.contains(&slot) != present {
                return Err(ReplayIndexGenerationError::InvalidRootSlot(name.clone()));
            }
        }
    }
    for row in rows {
        if !candidates.contains_key(&row.name) {
            return Err(ReplayIndexGenerationError::InvalidRootSlot(
                row.name.clone(),
            ));
        }
    }
    if explicit_global_this && !by_name.contains_key("globalThis") {
        return Err(ReplayIndexGenerationError::InvalidRootSlot(
            "globalThis".to_owned(),
        ));
    }
    Ok(())
}

struct NormalizedSourceRootCandidates {
    candidates: BTreeMap<String, crate::binder::declaration::SourceGlobalBindingCandidate>,
    namespace_contributors: BTreeSet<String>,
}

fn apply_merge_root_semantics(
    candidate: &mut crate::binder::declaration::SourceGlobalBindingCandidate,
    slots: BTreeSet<SourceBindingSlot>,
    ordinary_contributor: bool,
    namespace_instantiated: bool,
) {
    candidate.slots = slots;
    if namespace_instantiated {
        candidate.slots.insert(SourceBindingSlot::Value);
    }
    candidate.global_object_contributor = ordinary_contributor || namespace_instantiated;
}

fn normalize_source_root_candidates(
    mut candidates: BTreeMap<String, crate::binder::declaration::SourceGlobalBindingCandidate>,
    binder: &Binder,
) -> NormalizedSourceRootCandidates {
    let mut namespace_contributors = BTreeSet::new();
    for record in binder.namespaces.merges().filter(|record| {
        record.owner == crate::binder::namespace::DeclarationOwner::CompilationGlobal
    }) {
        if record.classification.disposition != crate::binder::namespace::MergeDisposition::Admitted
        {
            candidates.remove(record.name.as_ref());
            continue;
        }
        let Some(candidate) = candidates.get_mut(record.name.as_ref()) else {
            continue;
        };
        let mut slots = BTreeSet::new();
        let mut ordinary_contributor = false;
        for declaration in record.declarations.iter() {
            if declaration.spaces.value {
                slots.insert(SourceBindingSlot::Value);
            }
            if declaration.spaces.r#type {
                slots.insert(SourceBindingSlot::Type);
            }
            if declaration.spaces.namespace {
                slots.insert(SourceBindingSlot::Namespace);
            }
            ordinary_contributor |= declaration.kind
                == crate::binder::namespace::MergeDeclarationKind::Function
                || matches!(
                    declaration.syntax,
                    crate::binder::namespace::DeclarationSyntaxFacts::Variable(
                        crate::binder::namespace::VariableKind::Var
                    )
                );
        }
        let namespace = record
            .declarations
            .iter()
            .find_map(|declaration| declaration.namespace_fragment)
            .and_then(|fragment| binder.namespaces.fragment(fragment))
            .map(|fragment| fragment.namespace);
        let namespace_instantiated = namespace.is_some_and(|namespace| {
            binder.namespaces.aggregate_instance_state(namespace)
                == Some(crate::binder::namespace::NamespaceInstanceState::Instantiated)
        });
        apply_merge_root_semantics(
            candidate,
            slots,
            ordinary_contributor,
            namespace_instantiated,
        );
        if namespace_instantiated {
            namespace_contributors.insert(record.name.to_string());
        }
    }
    NormalizedSourceRootCandidates {
        candidates,
        namespace_contributors,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_collision_replay_index(
    trace: ReplayDependencyTrace,
    interner: &Interner,
    binder: &Binder,
    published: &PublishedTypeEnvironment,
    decl_types: &DeclTypes,
    namespace_terminals: &[FrozenNamespaceValueTerminalSnapshotRow],
    runtime: &FrozenCheckerRuntimeSnapshotParts,
    semantic_identities: Option<&LibrarySemanticIdentities>,
    canonical: &[CanonicalInput<'_>],
    parsed: &[oxc_parser::ParserReturn<'_>],
    module_scopes: &[ScopeId],
    class_declarations: &BTreeMap<ClassId, crate::binder::declaration::DeclId>,
    statement_keys: Vec<LibraryEventKey>,
    records: &[(LibraryEventKey, CheckerRecord)],
) -> Result<CollisionReplayIndex, InjectedProfileError> {
    fn add_owner_site(
        module_ordinals: &rustc_hash::FxHashMap<ScopeId, LibraryFileOrdinal>,
        sites: &mut BTreeSet<(ReplayOwner, LibraryFileOrdinal, u32, u32)>,
        invalid_sites: &mut u64,
        owner: ReplayOwner,
        module: ScopeId,
        span: Span,
    ) {
        if let Some(file_ordinal) = module_ordinals.get(&module).copied() {
            sites.insert((owner, file_ordinal, span.start, span.end));
        } else {
            *invalid_sites = invalid_sites.saturating_add(1);
        }
    }

    let module_ordinals = module_scopes
        .iter()
        .copied()
        .zip(canonical.iter().map(|input| input.file_ordinal))
        .collect::<rustc_hash::FxHashMap<_, _>>();
    let mut owners = Vec::new();
    owners.extend((0..binder.type_groups.len()).map(|index| {
        ReplayOwner::TypeGroup(TypeGroupId(
            u32::try_from(index).expect("type-group count fits u32"),
        ))
    }));
    owners.extend(
        (0..usize::try_from(binder.decl_count).unwrap_or(usize::MAX)).map(|index| {
            ReplayOwner::Value(ValueStorageId(
                u32::try_from(index).expect("value-storage count fits u32"),
            ))
        }),
    );
    owners.extend((0..binder.namespaces.len()).map(|index| {
        ReplayOwner::Namespace(NamespaceId(
            u32::try_from(index).expect("namespace count fits u32"),
        ))
    }));
    let class_terminals = published.classes().canonical_terminals().ok_or_else(|| {
        InjectedProfileError::CanonicalProjection(
            "replay generation saw a non-terminal class registry".to_owned(),
        )
    })?;
    owners.extend(
        class_terminals
            .iter()
            .map(|(class, _)| ReplayOwner::Class(*class)),
    );
    owners.push(ReplayOwner::GlobalObject);
    owners.extend(statement_keys.iter().copied().map(ReplayOwner::Statement));

    let mut invalid_sites = 0u64;
    let mut sites = BTreeSet::new();
    for index in 0..binder.type_groups.len() {
        let group = TypeGroupId(u32::try_from(index).expect("type-group count fits u32"));
        if let Some(row) = binder.type_groups.get(group) {
            for fragment in &row.fragments {
                add_owner_site(
                    &module_ordinals,
                    &mut sites,
                    &mut invalid_sites,
                    ReplayOwner::TypeGroup(group),
                    fragment.site.module,
                    fragment.site.declaration_span,
                );
            }
        }
    }
    for declaration in binder.declarations.iter() {
        if let Some(storage) = declaration.value_storage {
            add_owner_site(
                &module_ordinals,
                &mut sites,
                &mut invalid_sites,
                ReplayOwner::Value(storage),
                declaration.site.module,
                declaration.site.declaration_span,
            );
        }
    }
    for index in 0..binder.namespaces.len() {
        let namespace = NamespaceId(u32::try_from(index).expect("namespace count fits u32"));
        let Some(row) = binder.namespaces.get(namespace) else {
            continue;
        };
        for fragment_id in &row.fragments {
            let Some(fragment) = binder.namespaces.fragment(*fragment_id) else {
                invalid_sites = invalid_sites.saturating_add(1);
                continue;
            };
            let Some(declaration) = binder.declarations.get(fragment.declaration) else {
                invalid_sites = invalid_sites.saturating_add(1);
                continue;
            };
            add_owner_site(
                &module_ordinals,
                &mut sites,
                &mut invalid_sites,
                ReplayOwner::Namespace(namespace),
                declaration.site.module,
                declaration.site.declaration_span,
            );
            if let Some(storage) = binder.namespaces.standalone_value_storage(namespace) {
                add_owner_site(
                    &module_ordinals,
                    &mut sites,
                    &mut invalid_sites,
                    ReplayOwner::Value(storage),
                    declaration.site.module,
                    declaration.site.declaration_span,
                );
            }
        }
    }
    for (class, declaration) in class_declarations {
        if let Some(declaration) = binder.declarations.get(*declaration) {
            add_owner_site(
                &module_ordinals,
                &mut sites,
                &mut invalid_sites,
                ReplayOwner::Class(*class),
                declaration.site.module,
                declaration.site.declaration_span,
            );
        } else {
            invalid_sites = invalid_sites.saturating_add(1);
        }
    }

    let mut candidates = BTreeMap::new();
    let mut explicit_global_this = false;
    let mut contributor_sites = Vec::new();
    let mut explicit_global_this_sites = Vec::new();
    for ((input, parsed), module) in canonical.iter().zip(parsed).zip(module_scopes) {
        let provenance = source_global_binding_census_with_provenance(
            &parsed.program,
            ModuleBindingContext::for_program(&parsed.program, input.kind),
        );
        explicit_global_this |= provenance.census.explicit_global_this;
        contributor_sites.extend(
            provenance
                .contributor_sites
                .into_iter()
                .map(|site| (site.name, site.kind, input.file_ordinal, site.span)),
        );
        explicit_global_this_sites.extend(
            provenance
                .explicit_global_this_sites
                .into_iter()
                .map(|span| (input.file_ordinal, span)),
        );
        for (name, candidate) in provenance.census.candidates {
            let aggregate = candidates
                .entry(name)
                .or_insert_with(crate::binder::declaration::SourceGlobalBindingCandidate::default);
            aggregate.slots.extend(candidate.slots);
            aggregate.global_object_contributor |= candidate.global_object_contributor;
        }
        let _ = module;
    }
    if let Some(input) = canonical.first() {
        sites.insert((ReplayOwner::GlobalObject, input.file_ordinal, 0, 0));
    }
    for key in &statement_keys {
        sites.insert((
            ReplayOwner::Statement(*key),
            key.file_ordinal,
            key.source_start,
            key.source_start,
        ));
    }

    let root_rows = collect_root_rows(binder)
        .map_err(|error| InjectedProfileError::CanonicalProjection(format!("{error:?}")))?;
    let normalized = normalize_source_root_candidates(candidates, binder);
    let candidates = normalized.candidates;
    for (name, kind, file_ordinal, span) in contributor_sites {
        let admitted = match kind {
            SourceGlobalContributorKind::Ordinary => candidates.contains_key(&name),
            SourceGlobalContributorKind::Namespace => {
                normalized.namespace_contributors.contains(&name)
            }
        };
        if admitted {
            sites.insert((
                ReplayOwner::GlobalObject,
                file_ordinal,
                span.start,
                span.end,
            ));
        }
    }
    for (file_ordinal, span) in explicit_global_this_sites {
        sites.insert((
            ReplayOwner::GlobalObject,
            file_ordinal,
            span.start,
            span.end,
        ));
    }
    validate_root_census(&candidates, &root_rows, explicit_global_this)
        .map_err(InjectedProfileError::ReplayIndex)?;
    let mut roots = Vec::with_capacity(root_rows.len());
    {
        let _global_scope = trace.scope(ReplayOwner::GlobalObject);
        for row in root_rows {
            let candidate = candidates
                .get(&row.name)
                .expect("validated root rows have exact source provenance");
            let contributor = candidate.global_object_contributor;
            if contributor {
                if let Some(value) = row.value {
                    trace.demand(ReplayOwner::Value(value));
                }
                if let Some(ty) = row.ty {
                    trace.demand(ReplayOwner::TypeGroup(ty));
                }
                if let Some(namespace) = row.namespace {
                    trace.demand(ReplayOwner::Namespace(namespace));
                }
            }
            roots.push(ReplayRootSlot {
                explicit_global_this: explicit_global_this && row.name == "globalThis",
                name: row.name,
                value: row.value,
                ty: row.ty,
                namespace: row.namespace,
                global_object_contributor: contributor,
            });
        }
    }

    let mut record_bytes = BTreeMap::<LibraryEventKey, Vec<Vec<u8>>>::new();
    for (key, record) in records {
        let input = canonical_input_for_record(canonical, key.file_ordinal)?;
        record_bytes
            .entry(*key)
            .or_default()
            .push(canonical_record_bytes(canonical[input].source, record)?);
    }
    let baselines = owners
        .iter()
        .copied()
        .map(|owner| {
            let records = match owner {
                ReplayOwner::Statement(key) => {
                    record_bytes.get(&key).map_or(&[][..], Vec::as_slice)
                }
                _ => &[],
            };
            baseline_record(owner, records).map_err(InjectedProfileError::ReplayIndex)
        })
        .collect::<Result<Vec<_>, _>>()?;

    validate_terminal_class_dependencies(
        &trace,
        interner,
        ReplayTerminalValidationInputs {
            binder,
            published,
            decl_types,
            namespace_terminals,
            runtime,
            semantic_identities,
        },
    )?;
    let owner_sites = sites
        .into_iter()
        .map(|(owner, file_ordinal, start, end)| ReplayOwnerSite {
            owner,
            file_ordinal,
            span: Span::new(start, end),
        })
        .collect();
    trace
        .finish(
            owners,
            roots,
            owner_sites,
            statement_keys,
            baselines,
            invalid_sites,
        )
        .map_err(InjectedProfileError::ReplayIndex)
}

#[cfg(test)]
pub(crate) fn run_injected_profile(
    sources: &[InjectedLibrarySource<'_>],
) -> Result<InjectedProfileRun, InjectedProfileError> {
    compile_owned_injected_profile(sources).map(|(run, _)| run)
}

fn with_canonical_library_frontend<'source, Output>(
    sources: &[InjectedLibrarySource<'source>],
    consume: impl for<'ast> FnOnce(
        CanonicalLibraryFrontend<'source, 'ast>,
    ) -> Result<Output, InjectedProfileError>,
) -> Result<Output, InjectedProfileError> {
    #[cfg(test)]
    let parse_started = Instant::now();
    let canonical = canonical_inputs(sources)?;
    let allocators = (0..canonical.len())
        .map(|_| Allocator::default())
        .collect::<Vec<_>>();
    let parsed_and_claims = allocators
        .iter()
        .zip(&canonical)
        .map(|(allocator, input)| {
            let source_type = if input.kind.is_declaration() {
                SourceType::d_ts()
            } else {
                SourceType::ts()
            };
            let parsed = Parser::new(allocator, input.source, source_type).parse();
            if parsed.panicked {
                return Err(InjectedProfileError::Parse {
                    file_ordinal: input.file_ordinal,
                    messages: if parsed.diagnostics.is_empty() {
                        vec!["parser panicked without a diagnostic".to_owned()]
                    } else {
                        parsed
                            .diagnostics
                            .iter()
                            .map(|diagnostic| format!("{diagnostic:?}"))
                            .collect()
                    },
                });
            }
            let mut claims = Vec::with_capacity(parsed.diagnostics.len());
            for diagnostic in &parsed.diagnostics {
                let code_is_ts1319 = diagnostic.code.scope.as_deref() == Some("TS")
                    && diagnostic.code.number.as_deref() == Some("1319");
                let [label] = diagnostic.labels.as_slice() else {
                    return Err(InjectedProfileError::Parse {
                        file_ordinal: input.file_ordinal,
                        messages: parsed
                            .diagnostics
                            .iter()
                            .map(|diagnostic| format!("{diagnostic:?}"))
                            .collect(),
                    });
                };
                let start = label.offset();
                let Some(end) = start.checked_add(label.len()) else {
                    return Err(InjectedProfileError::Parse {
                        file_ordinal: input.file_ordinal,
                        messages: parsed
                            .diagnostics
                            .iter()
                            .map(|diagnostic| format!("{diagnostic:?}"))
                            .collect(),
                    });
                };
                if !code_is_ts1319 {
                    return Err(InjectedProfileError::Parse {
                        file_ordinal: input.file_ordinal,
                        messages: parsed
                            .diagnostics
                            .iter()
                            .map(|diagnostic| format!("{diagnostic:?}"))
                            .collect(),
                    });
                }
                claims.push(ParserExportClaim {
                    file_ordinal: input.file_ordinal,
                    span: Span::new(start, end),
                });
            }
            Ok((parsed, claims))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (parsed, claims): (Vec<_>, Vec<_>) = parsed_and_claims.into_iter().unzip();
    let parser_export_claims = claims.into_iter().flatten().collect::<Vec<_>>();
    #[cfg(test)]
    let parse_elapsed = parse_started.elapsed();

    #[cfg(test)]
    let bind_started = Instant::now();
    let units = parsed
        .iter()
        .zip(&canonical)
        .map(|(parsed, input)| {
            (
                &parsed.program,
                CompilationUnit {
                    source: input.source_key,
                    origin: CompilationOrigin::Library(input.file_ordinal),
                    binding: ModuleBindingContext::for_program(&parsed.program, input.kind),
                },
            )
        })
        .collect::<Vec<_>>();
    let prelude_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
    let mut builder = ProjectBinderBuilder::new(&prelude.program);
    let module_scopes = builder
        .try_add_library_modules(&units)
        .map_err(|error| InjectedProfileError::Binder(error.to_string()))?;
    let binder = builder.finish(module_scopes.last().copied().unwrap_or(ScopeId(0)));
    validate_parser_export_claims(&binder, parser_export_claims, canonical[0].file_ordinal)?;
    let semantic_scopes = units
        .iter()
        .zip(module_scopes.iter().copied())
        .map(|((_, unit), module)| {
            if unit.binding.external_module {
                module
            } else {
                binder.compilation_global
            }
        })
        .collect::<Vec<_>>();
    #[cfg(test)]
    let bind_elapsed = bind_started.elapsed();
    #[cfg(test)]
    {
        CANONICAL_FRONTEND_ENTRIES.set(CANONICAL_FRONTEND_ENTRIES.get().saturating_add(1));
        CANONICAL_FRONTEND_PARSE_UNITS.set(
            CANONICAL_FRONTEND_PARSE_UNITS
                .get()
                .saturating_add(u64::try_from(parsed.len()).unwrap_or(u64::MAX)),
        );
        CANONICAL_FRONTEND_BIND_BATCHES
            .set(CANONICAL_FRONTEND_BIND_BATCHES.get().saturating_add(1));
        CANONICAL_FRONTEND_BIND_UNITS.set(
            CANONICAL_FRONTEND_BIND_UNITS
                .get()
                .saturating_add(u64::try_from(module_scopes.len()).unwrap_or(u64::MAX)),
        );
    }
    consume(CanonicalLibraryFrontend {
        canonical,
        parsed,
        binder,
        module_scopes,
        semantic_scopes,
        #[cfg(test)]
        parse_elapsed,
        #[cfg(test)]
        bind_elapsed,
    })
}

pub(crate) fn compile_owned_injected_profile(
    sources: &[InjectedLibrarySource<'_>],
) -> Result<(InjectedProfileRun, OwnedLibraryRuntimeState), InjectedProfileError> {
    with_canonical_library_frontend(sources, compile_owned_injected_frontend)
}

fn compile_owned_injected_frontend(
    frontend: CanonicalLibraryFrontend<'_, '_>,
) -> Result<(InjectedProfileRun, OwnedLibraryRuntimeState), InjectedProfileError> {
    let CanonicalLibraryFrontend {
        canonical,
        parsed,
        binder,
        module_scopes,
        semantic_scopes,
        #[cfg(test)]
        parse_elapsed,
        #[cfg(test)]
        bind_elapsed,
    } = frontend;
    #[cfg(test)]
    CANONICAL_FRONTEND_FULL_PRODUCTS.set(CANONICAL_FRONTEND_FULL_PRODUCTS.get().saturating_add(1));

    #[cfg(test)]
    let reserve_fill_started = Instant::now();
    let mut ledger = LibraryEventLedger::default();
    let mut lexical_events: LexicalReservations<LibraryRecordTicket> =
        LexicalReservations::default();
    for (input, parsed) in canonical.iter().zip(&parsed) {
        lexical_events
            .reserve_library_program(input.file_ordinal, &parsed.program, &mut ledger)
            .map_err(InjectedProfileError::Reporting)?;
    }

    let mut interner = Interner::with_intrinsics();
    let mut next_type_param = 0;
    let mut next_class_id = 0;
    let mut type_decls: super::context::TypeDeclTable<'_> = Vec::new().into();
    let mut type_resolved: super::context::TypeResolvedTable =
        vec![None; binder.type_groups.len()].into();
    let declaration_spans = super::ModuleDeclarationSpans::index(&binder);
    for ((input, parsed), scope) in canonical
        .iter()
        .zip(&parsed)
        .zip(module_scopes.iter().copied())
    {
        reserve_type_decls(
            &mut interner,
            &binder,
            scope,
            &parsed.program,
            &mut next_type_param,
            &mut next_class_id,
            &mut type_decls,
            &mut type_resolved,
        );
        lexical_events.attach_library_declaration_owners(
            input.file_ordinal,
            &binder,
            scope,
            &parsed.program,
            &declaration_spans,
        );
        lexical_events.attach_library_class_bindings(
            input.file_ordinal,
            &binder,
            scope,
            &parsed.program,
            &type_decls,
        );
    }
    lexical_events
        .reserve_callable_type_params(&mut next_type_param)
        .map_err(|error| InjectedProfileError::Reservation(format!("{error:?}")))?;

    let replay_class_declarations = lexical_events
        .classes()
        .iter()
        .filter_map(|reservation| {
            let binding = reservation.binding.as_ref()?;
            let TypeDecl::Class { declaration, .. } = type_decls.get(binding.type_decl.index())?
            else {
                return None;
            };
            Some((binding.class_id, *declaration))
        })
        .collect::<BTreeMap<_, _>>();

    let pending_tickets = lexical_events.library_semantic_tickets();
    let mut pass = build_pass_with_tickets(
        &mut interner,
        &binder,
        type_decls,
        type_resolved,
        DeclTypes::new(binder.decl_count),
        next_type_param,
        PassReportingPlan {
            reporting: PassReporting {
                source: library_unit(canonical[0].file_ordinal),
                lexical_events,
                suppress_effects: false,
            },
            pending_tickets,
            ticket_key: library_record_ticket_key,
        },
    );
    pass.replay_trace = Some(ReplayDependencyTrace::new(ledger.replay_ticket_owners()));

    let declaration_count = pass.type_decls.len();
    pass.fill_type_decls_range(binder.module, 0, declaration_count);
    #[cfg(test)]
    let reserve_fill_elapsed = reserve_fill_started.elapsed();

    #[cfg(test)]
    let publication_validation_started = Instant::now();
    let module_programs = module_scopes
        .iter()
        .copied()
        .zip(parsed.iter())
        .map(|(scope, parsed)| (scope, parsed.program.body.as_slice()))
        .collect::<Vec<_>>();
    pass.prepare_project_attached_namespace_values(&module_programs);
    pass.prepare_project_standalone_namespace_values(&module_programs);
    pass.publish_class_surfaces();
    pass.finalize_standalone_namespace_values();
    pass.precompute_standalone_namespace_value_aliases(&module_programs);
    pass.fill_pending_interfaces_range(binder.module, 0, declaration_count);
    let publication_validations = pass.publish_type_groups();
    pass.validate_published_class_surfaces();
    #[cfg(test)]
    let lexical_source_units = pass
        .lexical_events
        .library_lexical_evidence()
        .iter()
        .copied()
        .collect::<Vec<_>>();
    #[cfg(test)]
    let publication_validation_elapsed = publication_validation_started.elapsed();

    #[cfg(test)]
    let statement_check_started = Instant::now();
    let mut pass_source_units = Vec::with_capacity(canonical.len());
    for (((input, parsed), module), semantic_scope) in canonical
        .iter()
        .zip(&parsed)
        .zip(module_scopes.iter().copied())
        .zip(semantic_scopes.iter().copied())
    {
        pass.current_module = module;
        pass.current_source = library_unit(input.file_ordinal);
        pass_source_units.push(pass.current_source);
        pass.build_flow_graph(semantic_scope, &parsed.program.body);
        pass.check_statements(semantic_scope, &parsed.program.body);
    }
    let batches = finish_semantic_effects(&mut pass);
    let semantic_identities = LibrarySemanticIdentities::select(
        &binder,
        pass.type_environment.published(),
        pass.interner.store(),
    );
    #[cfg(test)]
    let (global_types, module_types) = collect_type_probes(
        &binder,
        pass.type_environment.published(),
        pass.interner.store(),
        &canonical,
        &module_scopes,
    );
    #[cfg(test)]
    let global_values = collect_value_probes(
        &binder,
        &pass.decl_types,
        pass.interner.store(),
        &pass.namespace_values,
    );
    LibrarySemanticReportingAdapter::new(&mut ledger)
        .complete_semantic_batches(batches)
        .map_err(InjectedProfileError::Reporting)?;
    let reporting_receipts = LibraryReportingConsumer::new(&mut ledger)
        .consume_binder_outcomes(&binder)
        .map_err(InjectedProfileError::Reporting)?;
    #[cfg(not(test))]
    let _ = &reporting_receipts;
    let snapshot = ledger.snapshot();
    let statement_keys = ledger
        .replay_ticket_owners()
        .into_values()
        .collect::<Vec<_>>();
    let library_records = ledger.finish().map_err(InjectedProfileError::Reporting)?;
    let evidence = canonical_library_evidence(&canonical, &library_records)?;
    #[cfg(test)]
    let statement_check_elapsed = statement_check_started.elapsed();

    let namespace_terminals = pass
        .namespace_values
        .try_freeze_terminals()
        .map_err(|message| InjectedProfileError::CanonicalProjection(message.to_owned()))?;
    let namespace_terminal_rows = namespace_terminals
        .snapshot_parts()
        .map_err(|message| InjectedProfileError::CanonicalProjection(message.to_owned()))?;
    let replay_trace = pass.replay_trace.take().ok_or_else(|| {
        InjectedProfileError::CanonicalProjection(
            "source library compilation lost its replay trace".to_owned(),
        )
    })?;
    let super::context::Pass {
        type_environment,
        decl_types,
        next_type_param,
        class_application_parameters,
        class_new_metadata,
        class_parents,
        class_value_aliases,
        class_value_bindings,
        standalone_namespace_value_aliases,
        class_names,
        function_groups,
        ..
    } = pass;
    let super::type_groups::TypeEnvironmentState::Published(published_types) = type_environment
    else {
        return Err(InjectedProfileError::CanonicalProjection(
            "owned library runtime requires a published environment".to_owned(),
        ));
    };
    let named_function_symbols = function_groups.frozen_symbols();
    let runtime = FrozenCheckerRuntimeMetadata {
        class_application_parameters,
        class_new_metadata,
        class_parents,
        class_value_aliases,
        class_value_bindings,
        standalone_namespace_value_aliases,
        class_names,
        namespace_terminals,
        named_function_symbols: named_function_symbols.into(),
    };
    let runtime_parts = runtime
        .snapshot_parts()
        .map_err(|message| InjectedProfileError::CanonicalProjection(message.to_owned()))?;
    let selected_semantic_identities = semantic_identities
        .all_ready()
        .then_some(semantic_identities.clone());
    let replay_index = build_collision_replay_index(
        replay_trace,
        &interner,
        &binder,
        &published_types,
        &decl_types,
        &namespace_terminal_rows,
        &runtime_parts,
        selected_semantic_identities.as_ref(),
        &canonical,
        &parsed,
        &module_scopes,
        &replay_class_declarations,
        statement_keys,
        &library_records,
    )?;
    let replay_roots = collect_root_rows(&binder)
        .map_err(|error| InjectedProfileError::CanonicalProjection(error.to_string()))?
        .into_iter()
        .map(|row| (row.name, row.value, row.ty, row.namespace))
        .collect::<Vec<_>>();
    let replay_index = admit_generated_collision_replay_index(
        replay_index,
        ReplayIndexAdmissionLimits {
            type_groups: binder.type_groups.len(),
            value_storages: decl_types.len(),
            namespaces: binder.namespaces.len(),
            classes: usize::try_from(next_class_id)
                .map_err(|_| InjectedProfileError::SourceKeyOverflow)?,
            source_files: canonical
                .iter()
                .map(|input| input.file_ordinal.index().saturating_add(1))
                .max()
                .unwrap_or(0),
            roots: &replay_roots,
        },
        None,
    )
    .map_err(InjectedProfileError::ReplayIndexAdmission)?;
    let runtime_state = OwnedLibraryRuntimeState {
        interner,
        binder,
        published_types,
        decl_types,
        semantic_identities: selected_semantic_identities,
        runtime,
        next_type_param,
        next_class_id,
        source_file_count: u32::try_from(canonical.len())
            .map_err(|_| InjectedProfileError::SourceKeyOverflow)?,
        replay_index: Some(Box::new(replay_index)),
    };

    let run = InjectedProfileRun {
        phase_counts: LibraryPhaseCounts {
            parse_units: parsed.len(),
            bind_units: module_scopes.len(),
            reserved_records: snapshot.reserved_records,
            filled_records: snapshot.filled_records,
            publication_validations,
            statement_check_units: pass_source_units.len(),
        },
        #[cfg(test)]
        phase_timings: LibraryPhaseTimings {
            parse: parse_elapsed,
            bind: bind_elapsed,
            reserve_fill: reserve_fill_elapsed,
            publication_validation: publication_validation_elapsed,
            statement_check: statement_check_elapsed,
        },
        #[cfg(test)]
        reserved_file_ordinals: snapshot.reserved_file_ordinals,
        #[cfg(test)]
        reporting_receipts,
        library_records,
        evidence,
        #[cfg(test)]
        pass_source_units,
        #[cfg(test)]
        lexical_source_units,
        #[cfg(test)]
        global_types,
        #[cfg(test)]
        module_types,
        #[cfg(test)]
        global_values,
        #[cfg(test)]
        semantic_identities,
    };
    Ok((run, runtime_state))
}

pub(crate) fn compile_library_binder_checkpoint(
    sources: &[InjectedLibrarySource<'_>],
) -> Result<LibraryBinderCheckpoint, InjectedProfileError> {
    with_canonical_library_frontend(sources, |frontend| {
        #[cfg(test)]
        CANONICAL_FRONTEND_CHECKPOINT_PRODUCTS.set(
            CANONICAL_FRONTEND_CHECKPOINT_PRODUCTS
                .get()
                .saturating_add(1),
        );
        let CanonicalLibraryFrontend {
            canonical,
            binder,
            module_scopes,
            ..
        } = frontend;
        let library_units = canonical
            .iter()
            .zip(module_scopes)
            .map(|(input, module)| LibraryBinderUnit {
                ordinal: input.file_ordinal,
                source: input.source_key,
                module,
            })
            .collect();
        Ok(LibraryBinderCheckpoint::new(binder, library_units))
    })
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BinderContinuationModuleSourceForTest {
    pub(crate) module: ScopeId,
    pub(crate) source: crate::binder::namespace::SourceUnitKey,
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct LibraryBinderContinuationForTest {
    pub(crate) bound: super::BoundProjectBinder,
    pub(crate) checkpoint_ends: LibraryBinderCheckpointEnds,
    pub(crate) ends: LibraryBinderCheckpointEnds,
    pub(crate) array_symbol_before_augmentation: SymbolId,
    pub(crate) array_type_group_before_augmentation: TypeGroupId,
    pub(crate) array_symbol_after_augmentation: SymbolId,
    pub(crate) array_type_group_after_augmentation: TypeGroupId,
    pub(crate) consumer_array_type_group: TypeGroupId,
    pub(crate) augmentation_declaration: DeclId,
    pub(crate) appended_scopes: Vec<ScopeId>,
    pub(crate) appended_symbols: Vec<SymbolId>,
    pub(crate) appended_declarations: Vec<DeclId>,
    pub(crate) appended_type_groups: Vec<TypeGroupId>,
    pub(crate) appended_namespaces: Vec<NamespaceId>,
    pub(crate) appended_value_storages: Vec<ValueStorageId>,
    pub(crate) appended_module_sources: Vec<BinderContinuationModuleSourceForTest>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProjectBindingLookupForTest {
    pub(crate) symbol: SymbolId,
    pub(crate) value: Option<ValueStorageId>,
    pub(crate) type_group: Option<TypeGroupId>,
    pub(crate) namespace: Option<NamespaceId>,
    pub(crate) blocks_type_lookup: bool,
}

#[cfg(test)]
impl LibraryBinderContinuationForTest {
    pub(crate) fn normalized_per_path_binding_shape_for_test(&self) -> Vec<String> {
        self.bound
            .normalized
            .normalized_per_path_binding_shape
            .clone()
    }

    pub(crate) fn project_sources_for_test(&self) -> &[super::ProjectSourceBindingRow] {
        &self.bound.project_sources
    }

    fn project_module_for_test(&self, path: &str) -> Option<ScopeId> {
        self.bound
            .project_sources
            .iter()
            .find(|row| row.normalized_path == path)
            .map(|row| row.module)
    }

    pub(crate) fn lookup_binding_for_test(
        &self,
        path: &str,
        name: &str,
    ) -> Option<ProjectBindingLookupForTest> {
        let module = self.project_module_for_test(path)?;
        let symbol = self.bound.binder.graph.resolve(module, name)?;
        let slots = self.bound.binder.symbols.get(symbol)?;
        Some(ProjectBindingLookupForTest {
            symbol,
            value: slots.value,
            type_group: slots.ty,
            namespace: slots.ns,
            blocks_type_lookup: slots.blocks_type_lookup,
        })
    }

    pub(crate) fn import_placeholders_for_test(&self, path: &str) -> Vec<ValueStorageId> {
        let Some(module) = self.project_module_for_test(path) else {
            return Vec::new();
        };
        self.bound
            .module_scopes
            .iter()
            .position(|candidate| *candidate == module)
            .and_then(|index| self.bound.module_placeholders.get(index))
            .into_iter()
            .flatten()
            .filter_map(|placeholder| placeholder.value)
            .collect()
    }

    pub(crate) fn script_namespace_root_reservation_for_test(
        &self,
        name: &str,
    ) -> Option<SymbolId> {
        self.bound
            .binder
            .graph
            .get(self.bound.binder.script_namespace_root)?
            .lookup_local(name)
    }

    pub(crate) fn standalone_namespace_value_storage_for_test(
        &self,
        path: &str,
        name: &str,
    ) -> Option<ValueStorageId> {
        let namespace = self.lookup_binding_for_test(path, name)?.namespace?;
        self.bound
            .binder
            .namespaces
            .standalone_value_storage(namespace)
    }

    pub(crate) fn attached_namespace_value_disposition_for_test(
        &self,
        path: &str,
        name: &str,
    ) -> Option<crate::binder::namespace::NamespaceValueAttachmentDisposition> {
        let module = self.project_module_for_test(path)?;
        self.bound
            .binder
            .namespace_value_attachment(module, name)
            .map(|attachment| attachment.disposition)
    }
}

pub(crate) fn continue_library_project_binder(
    checkpoint: LibraryBinderCheckpoint,
    inputs: Vec<crate::driver::FileInput>,
) -> Result<super::BoundProjectBinder, String> {
    crate::driver::run_project_frontend(inputs, |_, units| {
        #[cfg(test)]
        record_user_source_parses_for_test(units.len());
        let bound = super::bind_library_checkpoint_project_programs(checkpoint, units)?;
        #[cfg(test)]
        {
            record_user_source_binds_for_test(units.len());
            super::record_continuation_project_binding_consumed_for_test();
        }
        Ok(bound)
    })
    .into_product()
}

#[cfg(test)]
pub(crate) fn continuation_receipt_for_test(
    checkpoint_ends: LibraryBinderCheckpointEnds,
    array_symbol_before_augmentation: SymbolId,
    array_type_group_before_augmentation: TypeGroupId,
    bound: super::BoundProjectBinder,
) -> Result<LibraryBinderContinuationForTest, String> {
    let binder = &bound.binder;
    let ends = binder.checkpoint_ends();
    let array_symbol_after_augmentation =
        binder
            .resolve_type(binder.compilation_global, "Array")
            .ok_or_else(|| "continued binder lost Array".to_owned())?;
    let array_type_group_after_augmentation = binder
        .symbols
        .get(array_symbol_after_augmentation)
        .and_then(|symbol| symbol.ty)
        .ok_or_else(|| "continued Array lost its type group".to_owned())?;
    // Generic project receipts retain the first appended declaration when Array is untouched.
    let augmentation_declaration = binder
        .type_groups
        .get(array_type_group_after_augmentation)
        .and_then(|group| {
            group
                .fragments
                .iter()
                .find(|fragment| fragment.declaration.index() >= checkpoint_ends.declarations)
        })
        .map(|fragment| fragment.declaration)
        .or_else(|| {
            (checkpoint_ends.declarations < ends.declarations).then(|| {
                DeclId(
                    u32::try_from(checkpoint_ends.declarations)
                        .expect("declaration prefix fits u32"),
                )
            })
        })
        .ok_or_else(|| "continued project appended no declaration".to_owned())?;
    let consumer_array_type_group = bound
        .module_scopes
        .last()
        .and_then(|module| binder.resolve_type(*module, "Array"))
        .and_then(|symbol| binder.symbols.get(symbol))
        .and_then(|symbol| symbol.ty)
        .ok_or_else(|| "consumer cannot resolve continued Array".to_owned())?;
    let appended_module_sources = bound
        .project_sources
        .iter()
        .map(|row| BinderContinuationModuleSourceForTest {
            module: row.module,
            source: row.source,
        })
        .collect();
    Ok(LibraryBinderContinuationForTest {
        bound,
        checkpoint_ends,
        ends,
        array_symbol_before_augmentation,
        array_type_group_before_augmentation,
        array_symbol_after_augmentation,
        array_type_group_after_augmentation,
        consumer_array_type_group,
        augmentation_declaration,
        appended_scopes: (checkpoint_ends.scopes..ends.scopes)
            .map(|id| ScopeId(u32::try_from(id).expect("scope id fits u32")))
            .collect(),
        appended_symbols: (checkpoint_ends.symbols..ends.symbols)
            .map(|id| SymbolId(u32::try_from(id).expect("symbol id fits u32")))
            .collect(),
        appended_declarations: (checkpoint_ends.declarations..ends.declarations)
            .map(|id| DeclId(u32::try_from(id).expect("declaration id fits u32")))
            .collect(),
        appended_type_groups: (checkpoint_ends.type_groups..ends.type_groups)
            .map(|id| TypeGroupId(u32::try_from(id).expect("type-group id fits u32")))
            .collect(),
        appended_namespaces: (checkpoint_ends.namespaces..ends.namespaces)
            .map(|id| NamespaceId(u32::try_from(id).expect("namespace id fits u32")))
            .collect(),
        appended_value_storages: (checkpoint_ends.value_storages..ends.value_storages)
            .map(|id| ValueStorageId(u32::try_from(id).expect("value-storage id fits u32")))
            .collect(),
        appended_module_sources,
    })
}

#[cfg(test)]
pub(crate) fn check_caller_certified_collision_free_project_with_owned_library(
    state: OwnedLibraryRuntimeState,
    inputs: Vec<crate::driver::FileInput>,
    expected_base: &OwnedLibraryRuntimeState,
) -> Result<OwnedBaseUserProjectRun, String> {
    let base_ends = state.identity_ends_for_test();
    let initial_visible_names = state.initial_visible_user_names_for_test();
    if !initial_visible_names.is_empty() {
        return Err("fresh project delta exposes prior user names".to_owned());
    }
    let cross_file = std::cell::RefCell::new(None);
    let final_identity = std::cell::RefCell::new(None);
    let state = std::cell::RefCell::new(Some(state));
    super::decls::reset_class_allocation_events_for_test();
    let reports = crate::driver::check_project_with_owned_checker_for_test(inputs, |units| {
        let state = state
            .borrow_mut()
            .take()
            .expect("owned project delta is consumed once");
        super::check_project_programs_with_owned_library(
            state,
            units,
            |binder, module_scopes| {
                let producer = module_scopes
                    .iter()
                    .copied()
                    .find_map(|scope| {
                        let ty = binder
                            .resolve_type(scope, "SharedShape")
                            .and_then(|symbol| binder.symbols.get(symbol))?
                            .ty?;
                        let value = binder
                            .resolve_value(scope, "sharedValue")
                            .and_then(|symbol| binder.symbols.get(symbol))?
                            .value?;
                        Some((scope, ty, value))
                    })
                    .expect("project producer exports both test bindings");
                let consumer = module_scopes
                    .iter()
                    .copied()
                    .filter(|scope| *scope != producer.0)
                    .find_map(|scope| {
                        let ty = binder
                            .resolve_type(scope, "SharedShape")
                            .and_then(|symbol| binder.symbols.get(symbol))?
                            .ty?;
                        let value = binder
                            .resolve_value(scope, "sharedValue")
                            .and_then(|symbol| binder.symbols.get(symbol))?
                            .value?;
                        Some((ty, value))
                    })
                    .expect("project consumer imports both test bindings");
                cross_file.replace(Some(OwnedBaseCrossFileWitness {
                    producer_type_group: producer.1,
                    consumer_type_group: consumer.0,
                    producer_value_storage: producer.2,
                    consumer_value_storage: consumer.1,
                }));
            },
            |pass, final_next_class_id| {
                final_identity.replace(Some(final_identity_witness(FinalIdentityInspection {
                    base_store_len: base_ends.store,
                    base_value_storage_len: base_ends.value_storages,
                    base_type_group_len: base_ends.type_groups,
                    base_namespace_len: base_ends.namespaces,
                    binder: pass.binder,
                    published: pass.type_environment.published(),
                    interner: pass.interner,
                    decl_types: &pass.decl_types,
                    next_type_param: pass.next_type_param,
                    next_class_id: final_next_class_id,
                    actual_class_ids: super::decls::class_allocation_events_for_test(),
                    references: final_reference_summary(pass, &base_ends),
                    base_row_clone_counts: expected_base
                        .final_base_family_clone_counts_for_test(pass, final_next_class_id),
                    local_rows_written: OwnedLibraryRuntimeState::final_local_rows_written_for_test(
                        pass,
                    ),
                })));
            },
        )
    });
    Ok(OwnedBaseUserProjectRun {
        reports,
        final_identity: final_identity
            .into_inner()
            .expect("owned project route captures final identities"),
        cross_file: cross_file
            .into_inner()
            .expect("owned project route captures cross-file identities"),
    })
}

#[cfg(test)]
pub(crate) fn check_caller_certified_collision_free_source_with_owned_library(
    state: OwnedLibraryRuntimeState,
    source: &str,
) -> Result<OwnedBaseUserRun, String> {
    check_caller_certified_collision_free_source_with_owned_library_impl(state, source, false, None)
}

#[cfg(test)]
pub(crate) fn check_caller_certified_collision_free_source_with_base_evidence(
    state: OwnedLibraryRuntimeState,
    source: &str,
    expected_base: &OwnedLibraryRuntimeState,
) -> Result<OwnedBaseUserRun, String> {
    check_caller_certified_collision_free_source_with_owned_library_impl(
        state,
        source,
        false,
        Some(expected_base),
    )
}

#[cfg(test)]
fn check_caller_certified_collision_free_source_with_owned_library_and_verify_prefix(
    state: OwnedLibraryRuntimeState,
    source: &str,
) -> Result<OwnedBaseUserRun, String> {
    check_caller_certified_collision_free_source_with_owned_library_impl(state, source, true, None)
}

#[cfg(test)]
fn check_caller_certified_collision_free_source_with_owned_library_impl(
    state: OwnedLibraryRuntimeState,
    source: &str,
    verify_store_prefix: bool,
    expected_base: Option<&OwnedLibraryRuntimeState>,
) -> Result<OwnedBaseUserRun, String> {
    // WU5 owns routing for suffixes that collide with the frozen global base.
    let parse_started = Instant::now();
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    if parsed.panicked || !parsed.diagnostics.is_empty() {
        return Err(format!(
            "user source parse failed: {:?}",
            parsed.diagnostics
        ));
    }
    record_user_source_parses_for_test(1);
    let parse = parse_started.elapsed();

    let OwnedLibraryRuntimeState {
        mut interner,
        binder,
        published_types,
        mut decl_types,
        semantic_identities,
        runtime,
        next_type_param,
        next_class_id,
        source_file_count: _,
        replay_index: _,
    } = state;
    let base_store_len = interner.store().len();
    let base_declared_recipe_len = interner.store().snapshot_declared_recipes().count();
    let base_store_digest = verify_store_prefix
        .then(|| store_prefix_digest(interner.store(), base_store_len, base_declared_recipe_len))
        .transpose()?;
    let base_type_group_count = binder.type_groups.len();
    let base_namespace_count = binder.namespaces.len();
    let base_decl_count = binder.decl_count;
    let base_identity_ends = OwnedBaseFinalIdentityEnds {
        store: base_store_len,
        declared_recipes: base_declared_recipe_len,
        type_params: usize::try_from(next_type_param).expect("base type parameter end fits usize"),
        classes: usize::try_from(next_class_id).expect("base class end fits usize"),
        scopes: binder.graph.snapshot_len(),
        symbols: binder.symbols.len(),
        declarations: binder.declarations.len(),
        type_groups: base_type_group_count,
        namespaces: base_namespace_count,
        value_storages: usize::try_from(base_decl_count).expect("base storage end fits usize"),
    };
    let base_max_source_key = binder.max_source_key().0;
    assert_eq!(
        decl_types.len(),
        usize::try_from(base_decl_count).expect("base declaration count fits usize"),
        "owned declaration types cover the complete binder prefix"
    );
    let base_array_group = binder
        .resolve_type(binder.compilation_global, "Array")
        .and_then(|symbol| binder.symbols.get(symbol))
        .and_then(|symbol| symbol.ty);
    let base_document_value = binder
        .resolve_value(binder.compilation_global, "document")
        .and_then(|symbol| binder.symbols.get(symbol))
        .and_then(|symbol| symbol.value);
    let bind_started = Instant::now();
    let (mut builder, source_key) = ProjectBinderBuilder::resume_frozen_library(binder);
    let unit = CompilationUnit::implementation(source_key, &parsed.program);
    builder.reserve_script_namespace_roots([(&parsed.program, unit)]);
    let (module, _) = builder.add_module(&parsed.program, &[], unit);
    let binder = builder
        .finish_frozen_library_continuation(module)
        .map_err(str::to_owned)?;
    decl_types.resize(binder.decl_count);
    let witness_source_key = source_key.0;
    let array_group_stable = base_array_group
        == binder
            .resolve_type(binder.module, "Array")
            .and_then(|symbol| binder.symbols.get(symbol))
            .and_then(|symbol| symbol.ty);
    let document_value_stable = base_document_value
        == binder
            .resolve_value(binder.module, "document")
            .and_then(|symbol| binder.symbols.get(symbol))
            .and_then(|symbol| symbol.value);
    let final_type_group_count = binder.type_groups.len();
    let final_decl_count = binder.decl_count;
    record_user_source_binds_for_test(1);
    let bind = bind_started.elapsed();

    let check_started = Instant::now();
    let final_identity = std::cell::RefCell::new(None);
    super::decls::reset_class_allocation_events_for_test();
    let result = check_bound_user_program_with_final_identity_inspector(
        &mut interner,
        binder,
        &parsed.program,
        BoundUserBase {
            published_types,
            library_semantic_identities: semantic_identities,
            lexical_array_alias: None,
            decl_types,
            next_type_param,
            next_class_id,
            runtime,
        },
        |_, _, _, _, _| {},
        |pass, final_next_class_id| {
            final_identity.replace(Some(final_identity_witness(FinalIdentityInspection {
                base_store_len,
                base_value_storage_len: usize::try_from(base_decl_count)
                    .expect("base storage end fits usize"),
                base_type_group_len: base_type_group_count,
                base_namespace_len: base_namespace_count,
                binder: pass.binder,
                published: pass.type_environment.published(),
                interner: pass.interner,
                decl_types: &pass.decl_types,
                next_type_param: pass.next_type_param,
                next_class_id: final_next_class_id,
                actual_class_ids: super::decls::class_allocation_events_for_test(),
                references: final_reference_summary(pass, &base_identity_ends),
                base_row_clone_counts: expected_base.map_or_else(BTreeMap::new, |expected_base| {
                    expected_base.final_base_family_clone_counts_for_test(pass, final_next_class_id)
                }),
                local_rows_written: OwnedLibraryRuntimeState::final_local_rows_written_for_test(
                    pass,
                ),
            })));
        },
    );
    record_user_source_checks_for_test(1);
    let check = check_started.elapsed();
    let final_identity = final_identity
        .into_inner()
        .expect("owned-base route captures final identities after effects");
    let final_store_len = final_identity.ends.store;
    let store_prefix_stable = base_store_digest
        .map(|base_store_digest| {
            store_prefix_digest(interner.store(), base_store_len, base_declared_recipe_len)
                .map(|final_store_digest| final_store_digest == base_store_digest)
        })
        .transpose()?;
    Ok(OwnedBaseUserRun {
        result,
        timings: OwnedBaseUserTimings { parse, bind, check },
        final_identity,
        witness: OwnedBaseContinuationWitness {
            base_store_len,
            final_store_len,
            base_type_group_count,
            final_type_group_count,
            base_decl_count,
            final_decl_count,
            source_key: witness_source_key,
            base_max_source_key,
            array_group_stable,
            document_value_stable,
            store_prefix_stable,
        },
    })
}

fn validate_parser_export_claims(
    binder: &Binder,
    parser_claims: Vec<ParserExportClaim>,
    fallback_file_ordinal: LibraryFileOrdinal,
) -> Result<(), InjectedProfileError> {
    let mut binder_claims = Vec::new();
    for context in binder.namespaces.export_contexts() {
        if context.syntax != ExportSyntaxDisposition::FutureTk1319 {
            continue;
        }
        let CompilationOrigin::Library(file_ordinal) = context.origin else {
            return Err(InjectedProfileError::Parse {
                file_ordinal: fallback_file_ordinal,
                messages: vec!["binder produced an unowned TK1319 export context".to_owned()],
            });
        };
        if context.kind != ExportContextKind::ExportDefault {
            return Err(InjectedProfileError::Parse {
                file_ordinal,
                messages: vec!["binder produced a non-default TK1319 export context".to_owned()],
            });
        }
        binder_claims.push(ParserExportClaim {
            file_ordinal,
            span: context.span,
        });
    }
    match_parser_export_claims(parser_claims, binder_claims)
}

fn match_parser_export_claims(
    mut parser_claims: Vec<ParserExportClaim>,
    binder_claims: Vec<ParserExportClaim>,
) -> Result<(), InjectedProfileError> {
    for binder_claim in binder_claims {
        let Some(index) = parser_claims
            .iter()
            .position(|parser_claim| *parser_claim == binder_claim)
        else {
            return Err(InjectedProfileError::Parse {
                file_ordinal: binder_claim.file_ordinal,
                messages: vec![format!(
                    "binder TK1319 claim has no parser owner at {}..{}",
                    binder_claim.span.start, binder_claim.span.end
                )],
            });
        };
        parser_claims.remove(index);
    }
    if let Some(parser_claim) = parser_claims.first() {
        return Err(InjectedProfileError::Parse {
            file_ordinal: parser_claim.file_ordinal,
            messages: vec![format!(
                "parser TS1319 claim has no binder owner at {}..{}",
                parser_claim.span.start, parser_claim.span.end
            )],
        });
    }
    Ok(())
}

fn canonical_inputs<'source>(
    sources: &[InjectedLibrarySource<'source>],
) -> Result<Vec<CanonicalInput<'source>>, InjectedProfileError> {
    if sources.is_empty() {
        return Err(InjectedProfileError::EmptyProfile);
    }
    let mut names = BTreeSet::new();
    let mut ordinals = BTreeSet::new();
    for source in sources {
        if source.name.is_empty() {
            return Err(InjectedProfileError::EmptyName {
                file_ordinal: source.file_ordinal,
            });
        }
        if !names.insert(source.name) {
            return Err(InjectedProfileError::DuplicateName(source.name.to_owned()));
        }
        if !ordinals.insert(source.file_ordinal) {
            return Err(InjectedProfileError::DuplicateFileOrdinal(
                source.file_ordinal,
            ));
        }
    }
    let mut sources = sources.iter().collect::<Vec<_>>();
    sources.sort_by_key(|source| source.file_ordinal);
    sources
        .into_iter()
        .enumerate()
        .map(|(index, source)| {
            let source_key = u32::try_from(index + 1)
                .map(exact_key)
                .map_err(|_| InjectedProfileError::SourceKeyOverflow)?;
            Ok(CanonicalInput {
                file_ordinal: source.file_ordinal,
                source: source.source,
                kind: source_file_kind(source.name),
                source_key,
            })
        })
        .collect()
}

#[cfg(test)]
fn collect_type_probes(
    binder: &Binder,
    published: &PublishedTypeEnvironment,
    store: &Store,
    canonical: &[CanonicalInput<'_>],
    module_scopes: &[ScopeId],
) -> (
    BTreeMap<String, TypeProbe>,
    BTreeMap<(LibraryFileOrdinal, String), TypeProbe>,
) {
    let mut globals = BTreeMap::new();
    let mut modules = BTreeMap::new();
    for group in binder.type_groups.iter() {
        let probe = type_probe(binder, published, store, group.id);
        let global_symbol = binder
            .graph
            .get(binder.compilation_global)
            .and_then(|scope| scope.lookup_local(&group.name))
            .and_then(|symbol| binder.symbols.get(symbol));
        if global_symbol.is_some_and(|symbol| symbol.ty == Some(group.id)) {
            globals.insert(group.name.clone(), probe.clone());
        }
        for (input, scope) in canonical.iter().zip(module_scopes) {
            let local_symbol = binder
                .graph
                .get(*scope)
                .and_then(|scope| scope.lookup_local(&group.name))
                .and_then(|symbol| binder.symbols.get(symbol));
            if local_symbol.is_some_and(|symbol| symbol.ty == Some(group.id)) {
                modules.insert((input.file_ordinal, group.name.clone()), probe.clone());
            }
        }
    }
    (globals, modules)
}

#[cfg(test)]
fn type_probe(
    binder: &Binder,
    published: &PublishedTypeEnvironment,
    store: &Store,
    identity: TypeGroupId,
) -> TypeProbe {
    let group = binder
        .type_groups
        .get(identity)
        .expect("published type probe has a binder group");
    let declaration_identities = group
        .fragments
        .iter()
        .filter_map(|fragment| {
            library_ordinal(
                binder
                    .namespaces
                    .compilation_origin_for_source(fragment.source)?,
            )
            .map(|file_ordinal| (file_ordinal, identity))
        })
        .collect::<Vec<_>>();
    let member_names = published
        .groups()
        .get(identity)
        .and_then(|terminal| match terminal {
            PublishedTypeGroupTerminal::Ready(group) => match group.surface {
                PublishedTypeGroupSurface::Template(ty) => Some(ty),
                PublishedTypeGroupSurface::Class(class) => {
                    match published.classes().published_class(class) {
                        DemandOutcome::Ready(surface) => Some(surface.instance_template()),
                        DemandOutcome::Exhausted(_) => None,
                    }
                }
            },
            PublishedTypeGroupTerminal::Unavailable(_) => None,
        })
        .and_then(|ty| store.object_type(ty))
        .map(|object| {
            object
                .properties
                .iter()
                .map(|property| property.name.clone())
                .collect()
        })
        .unwrap_or_default();
    TypeProbe {
        identity,
        declaration_identities,
        declaration_count: group.fragments.len(),
        member_names,
    }
}

#[cfg(test)]
fn collect_value_probes(
    binder: &Binder,
    decl_types: &DeclTypes,
    store: &Store,
    namespace_values: &NamespaceValueRegistry<LibraryRecordTicket>,
) -> BTreeMap<String, ValueProbe> {
    let mut probes = BTreeMap::new();
    for (symbol_id, symbol) in binder.symbols.iter() {
        if binder
            .graph
            .get(binder.compilation_global)
            .and_then(|scope| scope.lookup_local(&symbol.name))
            != Some(symbol_id)
        {
            continue;
        }
        if let Some(probe) = value_probe_for_symbol(
            binder,
            decl_types,
            store,
            namespace_values,
            binder.compilation_global,
            symbol_id,
        ) {
            probes.insert(symbol.name.clone(), probe);
        }
    }
    probes
}

#[cfg(test)]
fn value_probe_for_symbol(
    binder: &Binder,
    decl_types: &DeclTypes,
    store: &Store,
    namespace_values: &NamespaceValueRegistry<LibraryRecordTicket>,
    owner_scope: ScopeId,
    symbol_id: SymbolId,
) -> Option<ValueProbe> {
    let symbol = binder.symbols.get(symbol_id)?;
    let identity = symbol.value?;
    let participant_identities = symbol
        .declarations
        .iter()
        .filter_map(|declaration| {
            let declaration = binder.declarations.get(*declaration)?;
            origin_for_module(binder, declaration.site.module)
                .map(|file_ordinal| (file_ordinal, identity))
        })
        .collect::<Vec<_>>();
    let visible = decl_types.get(identity);
    let call_signature_count = visible
        .map(|ty| signature_ids(store, ty).len())
        .unwrap_or_default();
    let member_names = visible
        .and_then(|ty| store.object_type(ty))
        .map(|object| {
            object
                .properties
                .iter()
                .map(|property| property.name.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let callable_members = binder
        .namespace_value_attachment(owner_scope, &symbol.name)
        .map(|attachment| {
            attachment
                .members
                .into_iter()
                .filter(|member| member.kind == MergeDeclarationKind::Function)
                .filter_map(|member| {
                    let member_identity = member.value_storage?;
                    let file_ordinal = library_ordinal(member.origin)?;
                    let reservation =
                        namespace_values.namespace_function_reservation(member.declaration)?;
                    let property_ty = visible
                        .and_then(|ty| store.object_type(ty))
                        .and_then(|object| object.property(member.name))
                        .map(|property| property.ty)?;
                    let signature_ids = signature_ids(store, property_ty);
                    let signatures = signature_ids
                        .iter()
                        .filter_map(|signature| signature_probe(store, *signature))
                        .collect::<Vec<_>>();
                    Some(CallableMemberProbe {
                        name: member.name.to_owned(),
                        identity: member_identity,
                        source: library_unit(file_ordinal),
                        reservation_source: reservation.unit,
                        source_start: member.site.declaration_span.start,
                        call_signature_count: signature_ids.len(),
                        signatures,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Some(ValueProbe {
        identity,
        visible_type: visible,
        participant_identities,
        declaration_count: symbol.declarations.len(),
        call_signature_count,
        member_names,
        callable_members,
    })
}

#[cfg(test)]
fn signature_ids(store: &Store, ty: TypeId) -> Vec<TypeId> {
    match store.tag(ty) {
        TypeTag::Function => vec![ty],
        TypeTag::Object => store
            .object_type(ty)
            .map(|object| object.call_signatures.clone())
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
fn signature_probe(store: &Store, ty: TypeId) -> Option<SignatureProbe> {
    let signature = store.function_type(ty)?;
    Some(SignatureProbe {
        parameter_types: signature
            .params
            .iter()
            .map(|parameter| render_type(store, parameter.ty, false))
            .collect(),
        return_type: render_type(store, signature.ret, false),
    })
}

#[cfg(test)]
fn origin_for_module(binder: &Binder, module: ScopeId) -> Option<LibraryFileOrdinal> {
    binder
        .namespaces
        .source_units()
        .find(|unit| unit.module == module)
        .and_then(|unit| library_ordinal(unit.origin))
}

#[cfg(test)]
fn library_ordinal(origin: CompilationOrigin) -> Option<LibraryFileOrdinal> {
    match origin {
        CompilationOrigin::Library(file_ordinal) => Some(file_ordinal),
        CompilationOrigin::User(_) => None,
    }
}

#[cfg(test)]
fn release_phase_line(
    process: usize,
    registry_validation: Duration,
    timings: &LibraryPhaseTimings,
    total: Duration,
) -> String {
    format!(
        "typokat-wu0b-phase-v1 process={process} registry_validation_us={} parse_us={} bind_us={} reserve_fill_us={} publication_validation_us={} statement_check_us={} total_us={}",
        registry_validation.as_micros(),
        timings.parse.as_micros(),
        timings.bind.as_micros(),
        timings.reserve_fill.as_micros(),
        timings.publication_validation.as_micros(),
        timings.statement_check.as_micros(),
        total.as_micros(),
    )
}

#[cfg(test)]
struct ReleaseOutcomeLine {
    process: usize,
    file_count: usize,
    reserved_records: usize,
    filled_records: usize,
    publication_validations: usize,
    library_diagnostics: usize,
    library_incompletes: usize,
    tiny_parse_errors: usize,
    tiny_diagnostics: usize,
    tiny_incompletes: usize,
}

#[cfg(test)]
impl ReleaseOutcomeLine {
    fn render(&self) -> String {
        format!(
            "typokat-wu0b-outcome-v1 process={} file_count={} reserved_records={} filled_records={} publication_validations={} library_diagnostics={} library_incompletes={} tiny_parse_errors={} tiny_diagnostics={} tiny_incompletes={}",
            self.process,
            self.file_count,
            self.reserved_records,
            self.filled_records,
            self.publication_validations,
            self.library_diagnostics,
            self.library_incompletes,
            self.tiny_parse_errors,
            self.tiny_diagnostics,
            self.tiny_incompletes,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::checker::type_groups::PublishedTypeGroup;
    use crate::driver::check_source;
    use crate::library::profile::ExactLibraryProfile;

    fn assert_owned_terminal<T: Send + Sync + 'static>() {}

    /// Borrows the packaged 82-file profile as injected sources, the shape the compiler consumes.
    fn injected_packaged_sources(profile: &ExactLibraryProfile) -> Vec<InjectedLibrarySource<'_>> {
        profile
            .sources()
            .iter()
            .map(|source| InjectedLibrarySource {
                file_ordinal: source.ordinal(),
                name: source.name(),
                source: std::str::from_utf8(source.bytes())
                    .expect("packaged library sources are UTF-8"),
            })
            .collect()
    }

    const TINY_SOURCE: &str = "export const typokatLibraryProbe: number = 1;\n";
    const SNAPSHOT_SEMANTIC_LIBRARY: &str = r#"
        interface Array<T> { item: T; }
        interface ReadonlyArray<T> { item: T; }
        interface String { stringMarker: string; }
        interface Number { numberMarker: number; }
        interface Boolean { booleanMarker: boolean; }
        interface RegExp { regexpMarker: boolean; }
        interface Object { objectMarker: string; }
        interface Function { functionMarker: string; }
        interface CallableFunction extends Function { callableMarker: number; }
        interface SnapshotWitness { value: number; }
        declare class SnapshotBase { base: string; }
        declare class SnapshotCtorBase { protected constructor(); }
        declare class SnapshotInheritedCtor extends SnapshotCtorBase {}
        declare class SnapshotClass<T> extends SnapshotBase {
            constructor(value: T);
            value: T;
        }
        declare namespace SnapshotClass { export const tag: string; }
        declare namespace SnapshotSpace { export const enabled: boolean; }
        declare function snapshotNamed(value: number): string;
        declare namespace snapshotNamed { export const version: number; }
    "#;

    fn compile_semantic_snapshot_parts() -> OwnedLibraryRuntimeSnapshotParts {
        let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "snapshot-seam.d.ts",
            source: SNAPSHOT_SEMANTIC_LIBRARY,
        }])
        .expect("focused owned library profile");
        state.into_snapshot_parts().expect("extract snapshot parts")
    }

    fn compile_reservation_fixture(file_name: &str, source: &str) -> OwnedLibraryRuntimeState {
        compile_owned_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: file_name,
            source,
        }])
        .expect("reservation lifecycle fixture compiles")
        .1
    }

    fn published_template_type(state: &OwnedLibraryRuntimeState, name: &str) -> TypeId {
        let group = state
            .binder
            .type_groups
            .iter()
            .find(|group| group.name == name)
            .unwrap_or_else(|| panic!("missing type group {name}"));
        match state.published_types.groups().get(group.id) {
            Some(PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
                surface: PublishedTypeGroupSurface::Template(ty),
                ..
            })) => *ty,
            terminal => panic!("{name} did not publish a template: {terminal:?}"),
        }
    }

    fn published_object_property_type(
        state: &OwnedLibraryRuntimeState,
        owner: &str,
        property: &str,
    ) -> TypeId {
        let owner = published_template_type(state, owner);
        state
            .interner
            .store()
            .object_type(owner)
            .unwrap_or_else(|| panic!("{owner:?} is not an object template"))
            .property(property)
            .unwrap_or_else(|| panic!("{owner:?} has no {property} property"))
            .ty
    }

    fn store_type_reaches(
        state: &OwnedLibraryRuntimeState,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        const TYPE_DOMAIN: u8 = 1;
        let (references, _) = state.interner.snapshot_reference_records_for_test();
        let mut edges = rustc_hash::FxHashMap::<u32, Vec<u32>>::default();
        for (owner_domain, target_domain, _, owner, referenced) in references {
            if owner_domain == TYPE_DOMAIN && target_domain == TYPE_DOMAIN {
                edges.entry(owner).or_default().push(referenced);
            }
        }
        let mut visited = rustc_hash::FxHashSet::default();
        let mut pending = edges.get(&source.0).cloned().unwrap_or_default();
        while let Some(current) = pending.pop() {
            if current == target.0 {
                return true;
            }
            if visited.insert(current) {
                pending.extend(edges.get(&current).into_iter().flatten().copied());
            }
        }
        false
    }

    fn assert_strict_interner_snapshot(state: &OwnedLibraryRuntimeState, label: &str) {
        state
            .interner
            .encode_snapshot_bytes_for_test()
            .unwrap_or_else(|error| panic!("{label}: strict interner snapshot failed: {error:?}"));
    }

    fn assert_type_group_is_error_or_unavailable(state: &OwnedLibraryRuntimeState, name: &str) {
        let group = state
            .binder
            .type_groups
            .iter()
            .find(|group| group.name == name)
            .expect("fixture type group");
        match state.published_types.groups().get(group.id) {
            Some(PublishedTypeGroupTerminal::Unavailable(_)) => {}
            Some(PublishedTypeGroupTerminal::Ready(group)) => match group.surface {
                PublishedTypeGroupSurface::Template(ty)
                    if ty == state.interner.well_known().error => {}
                PublishedTypeGroupSurface::Template(ty) => panic!(
                    "{name} accidentally published TypeId {} ({:?}) instead of error/unavailable",
                    ty.0,
                    state.interner.store().tag(ty)
                ),
                PublishedTypeGroupSurface::Class(class) => {
                    panic!("{name} accidentally published class {class:?}")
                }
            },
            None => panic!("{name} has no published terminal"),
        }
    }

    fn strict_snapshot_failure(label: &str, source: &'static str, root: &str) -> Option<String> {
        let state = compile_reservation_fixture("reservation-lifecycle.d.ts", source);
        assert_type_group_is_error_or_unavailable(&state, root);
        state
            .interner
            .encode_snapshot_bytes_for_test()
            .err()
            .map(|error| format!("{label}: {error:?}"))
    }

    fn assert_valid_class_interface_merge(label: &str, source: &'static str) {
        let state = compile_reservation_fixture("class-interface-merge.d.ts", source);
        let group = state
            .binder
            .type_groups
            .iter()
            .find(|group| group.name == "MergedClassInterface")
            .expect("merged class/interface type group");
        let probe = type_probe(
            &state.binder,
            &state.published_types,
            state.interner.store(),
            group.id,
        );
        assert_eq!(probe.declaration_count, 2, "{label}: {probe:?}");
        assert_eq!(
            probe.member_names,
            ["classMember", "interfaceMember"],
            "{label}: {probe:?}"
        );
        assert!(
            matches!(
                state.published_types.groups().get(group.id),
                Some(PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
                    surface: PublishedTypeGroupSurface::Class(_),
                    ..
                }))
            ),
            "{label}: merged type group must publish a class surface"
        );
        state
            .interner
            .encode_snapshot_bytes_for_test()
            .unwrap_or_else(|error| panic!("{label}: strict interner encoding failed: {error:?}"));
    }

    fn replace_module_sources(
        parts: &mut OwnedLibraryRuntimeSnapshotParts,
        module_sources: rustc_hash::FxHashMap<ScopeId, crate::binder::namespace::SourceUnitKey>,
    ) {
        let placeholder = Binder::from_snapshot_parts(
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            ScopeId(0),
            ScopeId(0),
            ScopeId(0),
            ScopeId(0),
            0,
            0,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        let binder = std::mem::replace(&mut parts.binder, placeholder);
        parts.binder = Binder::from_snapshot_parts(
            binder.graph,
            binder.symbols,
            binder.declarations,
            binder.type_groups,
            binder.namespaces,
            binder.module,
            binder.prelude_module,
            binder.compilation_global,
            binder.script_namespace_root,
            binder.decl_count,
            binder.prelude_type_group_count,
            module_sources,
            binder.fn_scopes,
            binder.fn_decl_ids,
            binder.block_scopes,
        );
    }

    fn assert_snapshot_restore_error(
        parts: OwnedLibraryRuntimeSnapshotParts,
        expected: &'static str,
    ) {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            OwnedLibraryRuntimeState::from_snapshot_parts(parts)
        }));
        let Ok(result) = outcome else {
            panic!("snapshot corruption must return an error, not panic")
        };
        match result {
            Err(actual) => assert_eq!(actual, expected),
            Ok(_) => panic!("snapshot corruption unexpectedly restored"),
        }
    }

    #[test]
    fn injected_results_are_ast_free_owned_terminals() {
        assert_owned_terminal::<InjectedProfileRun>();
        assert_owned_terminal::<InjectedProfileError>();
        assert_owned_terminal::<LibraryPhaseTimings>();
        assert_owned_terminal::<OwnedLibraryRuntimeState>();
    }

    #[test]
    fn failed_conditional_alias_recovery_leaves_no_pending_reservation() {
        let source = r#"
            type FailedAwaited<T> = T extends null | undefined ? T :
                T extends symbol & {
                    then(onfulfilled: infer F, ...args: infer _): any;
                } ? F extends ((value: infer V, ...args: infer _) => any) ?
                    FailedAwaited<V> : never : T;
        "#;
        let failure = strict_snapshot_failure("FailedAwaited", source, "FailedAwaited");
        assert!(failure.is_none(), "{failure:#?}");
    }

    #[test]
    fn failed_mapped_alias_recovery_leaves_no_pending_reservation() {
        let source = r#"
            type FailedMapped<T> = { [K in FailedMapped<T>]: T };
        "#;
        let failure = strict_snapshot_failure("FailedMapped", source, "FailedMapped");
        assert!(failure.is_none(), "{failure:#?}");
    }

    #[test]
    fn conflicting_conditional_alias_reservations_close_in_both_orders() {
        let alias_first = r#"
            type ConditionalCollision<T> = T extends string ? string : number;
            interface ConditionalCollision<T> { interfaceMember: T; }
        "#;
        let interface_first = r#"
            interface ConditionalCollision<T> { interfaceMember: T; }
            type ConditionalCollision<T> = T extends string ? string : number;
        "#;
        let failures = [
            strict_snapshot_failure(
                "conditional-alias-first",
                alias_first,
                "ConditionalCollision",
            ),
            strict_snapshot_failure(
                "conditional-interface-first",
                interface_first,
                "ConditionalCollision",
            ),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn conflicting_mapped_alias_reservations_close_in_both_orders() {
        let alias_first = r#"
            type MappedCollision<T> = { [K in keyof T]: T[K] };
            interface MappedCollision<T> { interfaceMember: T; }
        "#;
        let interface_first = r#"
            interface MappedCollision<T> { interfaceMember: T; }
            type MappedCollision<T> = { [K in keyof T]: T[K] };
        "#;
        let failures = [
            strict_snapshot_failure("mapped-alias-first", alias_first, "MappedCollision"),
            strict_snapshot_failure("mapped-interface-first", interface_first, "MappedCollision"),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn conflicting_object_alias_reservations_close_in_both_orders() {
        let alias_first = r#"
            type ObjectCollision = { aliasMember: string };
            interface ObjectCollision { interfaceMember: number; }
        "#;
        let interface_first = r#"
            interface ObjectCollision { interfaceMember: number; }
            type ObjectCollision = { aliasMember: string };
        "#;
        let failures = [
            strict_snapshot_failure("object-alias-first", alias_first, "ObjectCollision"),
            strict_snapshot_failure("object-interface-first", interface_first, "ObjectCollision"),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn valid_class_interface_merge_closes_reservations_in_both_orders() {
        let class_first = r#"
            declare class MergedClassInterface { classMember: string; }
            interface MergedClassInterface { interfaceMember: number; }
        "#;
        let interface_first = r#"
            interface MergedClassInterface { interfaceMember: number; }
            declare class MergedClassInterface { classMember: string; }
        "#;

        assert_valid_class_interface_merge("class-first control", class_first);
        assert_valid_class_interface_merge("interface-first", interface_first);
    }

    #[test]
    fn acyclic_object_aliases_publish_one_structural_identity() {
        let state = compile_reservation_fixture(
            "acyclic-object-aliases.d.ts",
            r#"
                type FirstShape = {
                    value: number;
                    run(input: string): boolean;
                };
                type SecondShape = {
                    run(input: string): boolean;
                    value: number;
                };
            "#,
        );
        let first = published_template_type(&state, "FirstShape");
        let second = published_template_type(&state, "SecondShape");

        assert_eq!(first, second, "equal acyclic aliases must hash-cons");
        assert!(!store_type_reaches(&state, first, first));
        assert_strict_interner_snapshot(&state, "equal acyclic aliases");
    }

    #[test]
    fn acyclic_object_alias_reuses_an_existing_anonymous_shape() {
        let state = compile_reservation_fixture(
            "anonymous-object-collision.d.ts",
            r#"
                type Carrier = {
                    nested: {
                        value: number;
                        run(input: string): boolean;
                    };
                };
                type NamedShape = {
                    value: number;
                    run(input: string): boolean;
                };
            "#,
        );
        let anonymous = published_object_property_type(&state, "Carrier", "nested");
        let named = published_template_type(&state, "NamedShape");

        assert_eq!(named, anonymous, "named alias must reuse the anonymous row");
        assert_strict_interner_snapshot(&state, "anonymous shape collision");
    }

    #[test]
    fn recursive_object_aliases_retain_their_stable_reserved_roots() {
        let state = compile_reservation_fixture(
            "recursive-object-aliases.d.ts",
            r#"
                type SelfNode = { next: SelfNode | null };
                type MutualLeft = { right: MutualRight | null };
                type MutualRight = { left: MutualLeft | null };
                type NestedNode = {
                    next: (() => readonly NestedNode[]) | null;
                };
            "#,
        );
        let self_node = published_template_type(&state, "SelfNode");
        let mutual_left = published_template_type(&state, "MutualLeft");
        let mutual_right = published_template_type(&state, "MutualRight");
        let nested = published_template_type(&state, "NestedNode");

        assert!(store_type_reaches(&state, self_node, self_node));
        assert_ne!(mutual_left, mutual_right);
        assert!(store_type_reaches(&state, mutual_left, mutual_right));
        assert!(store_type_reaches(&state, mutual_right, mutual_left));
        assert!(store_type_reaches(&state, nested, nested));
        assert_strict_interner_snapshot(&state, "recursive aliases");
    }

    #[test]
    fn captured_object_alias_roots_are_never_remapped_behind_existing_edges() {
        let state = compile_reservation_fixture(
            "captured-object-aliases.d.ts",
            r#"
                type ForwardConsumer = { value: ForwardLater };
                type ForwardLater = { marker: number };

                interface InterfaceConsumer { value: InterfaceLater; }
                type InterfaceLater = { marker: string };

                type BackwardEarlier = { marker: boolean };
                type BackwardConsumer = { value: BackwardEarlier };
            "#,
        );

        for (consumer, dependency) in [
            ("ForwardConsumer", "ForwardLater"),
            ("InterfaceConsumer", "InterfaceLater"),
            ("BackwardConsumer", "BackwardEarlier"),
        ] {
            assert_eq!(
                published_object_property_type(&state, consumer, "value"),
                published_template_type(&state, dependency),
                "{consumer} must retain the published identity of {dependency}"
            );
        }
        assert_strict_interner_snapshot(&state, "captured alias roots");
    }

    #[test]
    fn type_parameter_defaults_follow_final_canonical_alias_identity() {
        let state = compile_reservation_fixture(
            "type-parameter-default-alias-capture.d.ts",
            r#"
                type FirstShape = { marker: number };
                type Box<T = NamedShape> = T;
                type NamedShape = { marker: number };
            "#,
        );
        let first = published_template_type(&state, "FirstShape");
        let named = published_template_type(&state, "NamedShape");
        let box_group = state
            .binder
            .type_groups
            .iter()
            .find(|group| group.name == "Box")
            .expect("Box type group");
        let Some(PublishedTypeGroupTerminal::Ready(box_surface)) =
            state.published_types.groups().get(box_group.id)
        else {
            panic!("Box did not publish a ready type group");
        };

        assert_eq!(first, named, "equal acyclic aliases must hash-cons");
        assert_strict_interner_snapshot(&state, "type parameter default alias capture");
        assert_eq!(
            box_surface.parameter_defaults,
            [PublishedTypeParameterDefault::Ready(named)],
            "published defaults must follow the final canonical alias identity"
        );
    }

    #[test]
    fn replay_index_covers_class_references_in_interface_parameter_defaults() {
        let state = compile_reservation_fixture(
            "replay-interface-class-default.d.ts",
            r#"
                declare class DefaultClass {}
                interface Box<T = DefaultClass> { value: T; }
            "#,
        );
        let replay = state
            .replay_index()
            .expect("source compiler retains its replay index");

        assert_eq!(replay.unowned_demand_count, 0);
        assert_eq!(replay.invalid_owner_site_count, 0);
        assert_eq!(replay.noncanonical_edge_count, 0);
        assert_eq!(replay.typed_reference_coverage_misses, 0);
    }

    #[test]
    fn replay_index_covers_class_references_copied_across_overload_rows() {
        let state = compile_reservation_fixture(
            "replay-overload-class-provenance.d.ts",
            r#"
                declare class DefaultClass {}
                declare function consume(value: DefaultClass): void;
                declare function consume(value: string): void;
            "#,
        );
        let replay = state
            .replay_index()
            .expect("source compiler retains its replay index");

        assert_eq!(replay.unowned_demand_count, 0);
        assert_eq!(replay.invalid_owner_site_count, 0);
        assert_eq!(replay.noncanonical_edge_count, 0);
        assert_eq!(replay.typed_reference_coverage_misses, 0);
    }

    #[test]
    fn terminal_class_validator_follows_declared_recipe_template_dependencies() {
        const DECLARED_RECIPE_DOMAIN: u8 = 10;
        const TYPE_DOMAIN: u8 = 1;
        const CLASS_DOMAIN: u8 = 3;

        let state = compile_reservation_fixture(
            "replay-declared-recipe-class-template.d.ts",
            r#"
                declare class RecipeDependency {}
                interface RecipeTemplate<T> {
                    item: T;
                    dependency: RecipeDependency;
                }
                interface RecipeConsumer {
                    nested: RecipeTemplate<string>[];
                }
            "#,
        );
        let consumer_group = state
            .binder
            .type_groups
            .iter()
            .find(|group| group.name == "RecipeConsumer")
            .expect("consumer type group")
            .id;
        let consumer_template = published_template_type(&state, "RecipeConsumer");
        let (store_references, _) = state.interner.snapshot_reference_records_for_test();
        let mut pending = vec![(TYPE_DOMAIN, consumer_template.0, false)];
        let mut visited = BTreeSet::new();
        let mut reaches_class_through_recipe = false;
        while let Some((domain, owner, crossed_recipe)) = pending.pop() {
            if !visited.insert((domain, owner, crossed_recipe)) {
                continue;
            }
            for &(owner_domain, target_domain, _, row, target) in &store_references {
                if owner_domain != domain || row != owner {
                    continue;
                }
                if target_domain == CLASS_DOMAIN && crossed_recipe {
                    reaches_class_through_recipe = true;
                    break;
                }
                if matches!(target_domain, TYPE_DOMAIN | DECLARED_RECIPE_DOMAIN) {
                    pending.push((
                        target_domain,
                        target,
                        crossed_recipe || target_domain == DECLARED_RECIPE_DOMAIN,
                    ));
                }
            }
            if reaches_class_through_recipe {
                break;
            }
        }
        assert!(
            reaches_class_through_recipe,
            "fixture must expose Type -> Recipe -> Type/Class dependency reachability"
        );

        let trace = ReplayDependencyTrace::new(BTreeMap::new());
        let replay = state
            .replay_index()
            .expect("source compiler retains its replay index");
        for edge in &replay.reverse_edges {
            if edge.consumer == ReplayOwner::TypeGroup(consumer_group) {
                continue;
            }
            trace.enter(edge.consumer);
            trace.demand(edge.dependency);
            trace.leave(edge.consumer);
        }
        let runtime = state
            .runtime
            .snapshot_parts()
            .expect("runtime snapshot parts");
        validate_terminal_class_dependencies(
            &trace,
            &state.interner,
            ReplayTerminalValidationInputs {
                binder: &state.binder,
                published: &state.published_types,
                decl_types: &state.decl_types,
                namespace_terminals: &runtime.namespace_terminals,
                runtime: &runtime,
                semantic_identities: state.semantic_identities.as_ref(),
            },
        )
        .expect("terminal validation enumerates canonical references");
        let error = trace
            .finish(
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                0,
            )
            .expect_err("omitting the consumer dependency must fail typed coverage");
        assert!(
            matches!(
                error,
                ReplayIndexGenerationError::TypedReferenceCoverage { .. }
            ),
            "declared recipe template dependency was invisible to terminal validation: {error:?}"
        );
    }

    #[test]
    fn terminal_class_validator_preserves_cycles_duplicates_and_multiple_classes() {
        const TYPE_DOMAIN: u8 = 1;
        const DECLARED_RECIPE_DOMAIN: u8 = 10;

        let owner = ReplayOwner::TypeGroup(TypeGroupId(0));
        let classes = [ClassId(7), ClassId(11)];
        let semantic_edges = BTreeMap::from([
            ((TYPE_DOMAIN, 0), vec![(DECLARED_RECIPE_DOMAIN, 10)]),
            (
                (DECLARED_RECIPE_DOMAIN, 10),
                vec![(TYPE_DOMAIN, 11), (TYPE_DOMAIN, 11)],
            ),
            ((TYPE_DOMAIN, 11), vec![(DECLARED_RECIPE_DOMAIN, 10)]),
        ]);
        let class_edges = BTreeMap::from([
            ((DECLARED_RECIPE_DOMAIN, 10), vec![classes[0], classes[0]]),
            ((TYPE_DOMAIN, 11), vec![classes[1]]),
        ]);
        let direct = BTreeMap::from([(owner, vec![TypeId(0)])]);

        let trace = ReplayDependencyTrace::default();
        trace.enter(owner);
        for class in classes {
            trace.demand(ReplayOwner::Class(class));
        }
        trace.leave(owner);
        require_terminal_class_dependency_closure(
            &trace,
            &semantic_edges,
            &class_edges,
            direct,
            None,
        );

        let partition = vec![
            owner,
            ReplayOwner::Class(classes[0]),
            ReplayOwner::Class(classes[1]),
        ];
        let sites = partition
            .iter()
            .copied()
            .map(|owner| ReplayOwnerSite {
                owner,
                file_ordinal: LibraryFileOrdinal::new(0),
                span: Span::new(0, 1),
            })
            .collect::<Vec<_>>();
        let baselines = partition
            .iter()
            .copied()
            .map(|owner| baseline_record(owner, &[]).expect("empty owner baseline"))
            .collect::<Vec<_>>();
        let replay = trace
            .finish(partition, Vec::new(), sites, Vec::new(), baselines, 0)
            .expect("cycles and duplicate edges retain both terminal classes");
        assert_eq!(
            replay
                .reverse_edges
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                ReplayReverseEdge {
                    dependency: ReplayOwner::Class(classes[0]),
                    consumer: owner,
                },
                ReplayReverseEdge {
                    dependency: ReplayOwner::Class(classes[1]),
                    consumer: owner,
                },
            ])
        );
    }

    #[test]
    fn terminal_class_validator_processes_a_shared_semantic_tail_once() {
        const TYPE_DOMAIN: u8 = 1;
        const DECLARED_RECIPE_DOMAIN: u8 = 10;
        const OWNER_COUNT: usize = 64;
        const SHARED_DEPTH: usize = 128;

        let owners = (0..OWNER_COUNT)
            .map(|index| {
                ReplayOwner::TypeGroup(TypeGroupId(
                    u32::try_from(index).expect("owner id fits u32"),
                ))
            })
            .collect::<Vec<_>>();
        let classes = [ClassId(7), ClassId(11), ClassId(13)];
        let shared_tail = (0..SHARED_DEPTH)
            .map(|index| {
                let domain = if index % 2 == 0 {
                    DECLARED_RECIPE_DOMAIN
                } else {
                    TYPE_DOMAIN
                };
                (
                    domain,
                    10_000 + u32::try_from(index).expect("semantic id fits u32"),
                )
            })
            .collect::<Vec<_>>();

        let mut semantic_edges = BTreeMap::<(u8, u32), Vec<(u8, u32)>>::new();
        let mut direct = BTreeMap::<ReplayOwner, Vec<TypeId>>::new();
        for (index, owner) in owners.iter().copied().enumerate() {
            let root = TypeId(u32::try_from(index).expect("root type id fits u32"));
            direct.insert(owner, vec![root]);
            semantic_edges
                .entry((TYPE_DOMAIN, root.0))
                .or_default()
                .push(shared_tail[0]);
        }
        for pair in shared_tail.windows(2) {
            semantic_edges
                .entry(pair[0])
                .or_default()
                .extend([pair[1], pair[1]]);
        }
        semantic_edges
            .entry(*shared_tail.last().expect("non-empty shared tail"))
            .or_default()
            .push(shared_tail[SHARED_DEPTH / 2]);

        let mut class_edges = BTreeMap::<(u8, u32), Vec<ClassId>>::new();
        class_edges
            .entry(shared_tail[SHARED_DEPTH / 4])
            .or_default()
            .extend([classes[0], classes[0]]);
        class_edges
            .entry(shared_tail[SHARED_DEPTH / 2])
            .or_default()
            .push(classes[1]);
        class_edges
            .entry(*shared_tail.last().expect("non-empty shared tail"))
            .or_default()
            .push(classes[2]);

        let trace = ReplayDependencyTrace::default();
        for owner in &owners {
            trace.enter(*owner);
            for class in classes {
                trace.demand(ReplayOwner::Class(class));
            }
            trace.leave(*owner);
        }
        let mut work = TerminalClassDependencyValidationWork::default();
        require_terminal_class_dependency_closure(
            &trace,
            &semantic_edges,
            &class_edges,
            direct,
            Some(&mut work),
        );

        let mut partition = owners.clone();
        partition.extend(classes.into_iter().map(ReplayOwner::Class));
        let sites = partition
            .iter()
            .copied()
            .map(|owner| ReplayOwnerSite {
                owner,
                file_ordinal: LibraryFileOrdinal::new(0),
                span: Span::new(0, 1),
            })
            .collect::<Vec<_>>();
        let baselines = partition
            .iter()
            .copied()
            .map(|owner| baseline_record(owner, &[]).expect("empty owner baseline"))
            .collect::<Vec<_>>();
        let replay = trace
            .finish(partition, Vec::new(), sites, Vec::new(), baselines, 0)
            .expect("cycles and duplicate semantic edges preserve the exact class oracle");
        let expected_edges = owners
            .iter()
            .flat_map(|owner| {
                classes.iter().map(move |class| ReplayReverseEdge {
                    dependency: ReplayOwner::Class(*class),
                    consumer: *owner,
                })
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            replay
                .reverse_edges
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>(),
            expected_edges,
            "shared-tail summarization must not change the owner-to-class oracle"
        );

        let unique_semantic_nodes = OWNER_COUNT + SHARED_DEPTH;
        assert_eq!(
            work.owner_root_summaries,
            u64::try_from(OWNER_COUNT).expect("owner count fits u64")
        );
        let linear_input_size = unique_semantic_nodes
            + semantic_edges.values().map(Vec::len).sum::<usize>()
            + class_edges.values().map(Vec::len).sum::<usize>()
            + OWNER_COUNT;
        let measured_work = work
            .owner_root_summaries
            .saturating_add(work.semantic_node_visits)
            .saturating_add(work.semantic_edge_probes)
            .saturating_add(work.class_edge_probes);
        assert!(
            measured_work
                <= 4 * u64::try_from(linear_input_size).expect("validation input fits u64"),
            "terminal dependency validation must stay O(V + E + owners), not owners * depth: \
             input={linear_input_size}, work={work:?}"
        );
    }

    #[test]
    fn terminal_class_validator_deduplicates_repeated_class_across_shared_tail() {
        const TYPE_DOMAIN: u8 = 1;
        const OWNER_COUNT: usize = 64;
        const SHARED_DEPTH: usize = 128;

        let owners = (0..OWNER_COUNT)
            .map(|index| {
                ReplayOwner::TypeGroup(TypeGroupId(
                    u32::try_from(index).expect("owner id fits u32"),
                ))
            })
            .collect::<Vec<_>>();
        let class = ClassId(7);
        let shared_tail = (0..SHARED_DEPTH)
            .map(|index| {
                (
                    TYPE_DOMAIN,
                    10_000 + u32::try_from(index).expect("semantic id fits u32"),
                )
            })
            .collect::<Vec<_>>();
        let mut semantic_edges = BTreeMap::<(u8, u32), Vec<(u8, u32)>>::new();
        let mut direct = BTreeMap::<ReplayOwner, Vec<TypeId>>::new();
        for (index, owner) in owners.iter().copied().enumerate() {
            let root = TypeId(u32::try_from(index).expect("root type id fits u32"));
            direct.insert(owner, vec![root]);
            semantic_edges.insert((TYPE_DOMAIN, root.0), vec![shared_tail[0]]);
        }
        for pair in shared_tail.windows(2) {
            semantic_edges.insert(pair[0], vec![pair[1]]);
        }
        let class_edges = shared_tail
            .iter()
            .copied()
            .map(|node| (node, vec![class]))
            .collect::<BTreeMap<_, _>>();

        let trace = ReplayDependencyTrace::default();
        for owner in &owners {
            trace.enter(*owner);
            trace.demand(ReplayOwner::Class(class));
            trace.leave(*owner);
        }
        let mut work = TerminalClassDependencyValidationWork::default();
        require_terminal_class_dependency_closure(
            &trace,
            &semantic_edges,
            &class_edges,
            direct,
            Some(&mut work),
        );

        let mut partition = owners.clone();
        partition.push(ReplayOwner::Class(class));
        let sites = partition
            .iter()
            .copied()
            .map(|owner| ReplayOwnerSite {
                owner,
                file_ordinal: LibraryFileOrdinal::new(0),
                span: Span::new(0, 1),
            })
            .collect::<Vec<_>>();
        let baselines = partition
            .iter()
            .copied()
            .map(|owner| baseline_record(owner, &[]).expect("empty owner baseline"))
            .collect::<Vec<_>>();
        let replay = trace
            .finish(partition, Vec::new(), sites, Vec::new(), baselines, 0)
            .expect("repeated class references retain their exact terminal-class oracle");
        let expected_edges = owners
            .iter()
            .map(|owner| ReplayReverseEdge {
                dependency: ReplayOwner::Class(class),
                consumer: *owner,
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            replay
                .reverse_edges
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>(),
            expected_edges
        );

        let semantic_node_count = OWNER_COUNT + SHARED_DEPTH;
        let semantic_edge_count = OWNER_COUNT + SHARED_DEPTH - 1;
        let class_edge_count = SHARED_DEPTH;
        let output_edge_count = OWNER_COUNT;
        let linear_input_and_output = semantic_node_count
            + semantic_edge_count
            + class_edge_count
            + output_edge_count
            + OWNER_COUNT;
        assert!(
            work.class_summary_items
                <= 6 * u64::try_from(linear_input_and_output)
                    .expect("validation input and output fit u64"),
            "repeated terminal classes must be deduplicated before owner expansion: \
             input+output={linear_input_and_output}, work={work:?}"
        );
    }

    #[test]
    fn terminal_class_validator_summary_work_is_output_sensitive_for_unique_chain() {
        const TYPE_DOMAIN: u8 = 1;
        const ROOTED_DEPTH: usize = 1_024;
        const DISCONNECTED_DEPTH: usize = 2_048;

        let owner = ReplayOwner::TypeGroup(TypeGroupId(0));
        let rooted_classes = (0..ROOTED_DEPTH)
            .map(|index| ClassId(10_000 + u32::try_from(index).expect("class id fits u32")))
            .collect::<Vec<_>>();
        let mut semantic_edges = BTreeMap::<(u8, u32), Vec<(u8, u32)>>::new();
        let mut class_edges = BTreeMap::<(u8, u32), Vec<ClassId>>::new();

        for (index, class) in rooted_classes.iter().copied().enumerate() {
            let node = (
                TYPE_DOMAIN,
                u32::try_from(index).expect("semantic id fits u32"),
            );
            class_edges.insert(node, vec![class]);
            if index + 1 < ROOTED_DEPTH {
                semantic_edges.insert(
                    node,
                    vec![(
                        TYPE_DOMAIN,
                        u32::try_from(index + 1).expect("semantic id fits u32"),
                    )],
                );
            }
        }

        for index in 0..DISCONNECTED_DEPTH {
            let id = 100_000 + u32::try_from(index).expect("semantic id fits u32");
            let node = (TYPE_DOMAIN, id);
            class_edges.insert(
                node,
                vec![ClassId(
                    200_000 + u32::try_from(index).expect("class id fits u32"),
                )],
            );
            if index + 1 < DISCONNECTED_DEPTH {
                semantic_edges.insert(node, vec![(TYPE_DOMAIN, id + 1)]);
            }
        }

        let trace = ReplayDependencyTrace::default();
        trace.enter(owner);
        for class in &rooted_classes {
            trace.demand(ReplayOwner::Class(*class));
        }
        trace.leave(owner);

        let mut work = TerminalClassDependencyValidationWork::default();
        require_terminal_class_dependency_closure(
            &trace,
            &semantic_edges,
            &class_edges,
            BTreeMap::from([(owner, vec![TypeId(0)])]),
            Some(&mut work),
        );

        let mut partition = vec![owner];
        partition.extend(rooted_classes.iter().copied().map(ReplayOwner::Class));
        let sites = partition
            .iter()
            .copied()
            .map(|owner| ReplayOwnerSite {
                owner,
                file_ordinal: LibraryFileOrdinal::new(0),
                span: Span::new(0, 1),
            })
            .collect::<Vec<_>>();
        let baselines = partition
            .iter()
            .copied()
            .map(|owner| baseline_record(owner, &[]).expect("empty owner baseline"))
            .collect::<Vec<_>>();
        let replay = trace
            .finish(partition, Vec::new(), sites, Vec::new(), baselines, 0)
            .expect("the rooted chain retains its exact terminal-class oracle");
        let expected_edges = rooted_classes
            .iter()
            .map(|class| ReplayReverseEdge {
                dependency: ReplayOwner::Class(*class),
                consumer: owner,
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            replay
                .reverse_edges
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>(),
            expected_edges,
            "the output oracle must contain exactly the rooted chain's classes"
        );

        let semantic_node_count = ROOTED_DEPTH + DISCONNECTED_DEPTH;
        let semantic_edge_count = (ROOTED_DEPTH - 1) + (DISCONNECTED_DEPTH - 1);
        let class_edge_count = semantic_node_count;
        let output_edge_count = ROOTED_DEPTH;
        let linear_input_and_output =
            semantic_node_count + semantic_edge_count + class_edge_count + output_edge_count + 1;
        assert!(
            work.class_summary_items
                <= 6 * u64::try_from(linear_input_and_output)
                    .expect("validation input and output fit u64"),
            "terminal-class summaries must be root-relevant and output-sensitive, not copy every \
             suffix's growing class set: input+output={linear_input_and_output}, work={work:?}"
        );
    }

    /// Shared *owner-expression* spine: a chain of `L` components each merging the
    /// same cross owner, so every spine component mints a fresh two-input
    /// `TerminalOwnerExpression::Union` whose owner set stays tiny (`{seed, cross}`).
    /// `M` sibling components hang off the tail, each with its own owner and its own
    /// terminal class, so each becomes a *separate* query root that is not an
    /// ancestor of any other. No spine union is ever a query root, so a
    /// summarizer that memoises only at roots re-walks the whole spine once per
    /// sibling — Θ(M · L) work for Θ(M) output.
    fn shared_union_spine_probe(
        spine_len: usize,
        siblings: usize,
        demand_first_sibling: bool,
    ) -> (u64, TerminalClassDependencyValidationWork) {
        const TYPE_DOMAIN: u8 = 1;
        let spine = |index: usize| {
            (
                TYPE_DOMAIN,
                1_000_000 + u32::try_from(index).expect("spine id fits u32"),
            )
        };
        let cross = (TYPE_DOMAIN, 2_000_000_u32);
        let sibling = |index: usize| {
            (
                TYPE_DOMAIN,
                3_000_000 + u32::try_from(index).expect("sibling id fits u32"),
            )
        };
        let sibling_root = |index: usize| {
            (
                TYPE_DOMAIN,
                4_000_000 + u32::try_from(index).expect("sibling root id fits u32"),
            )
        };

        let mut semantic_edges = BTreeMap::<(u8, u32), Vec<(u8, u32)>>::new();
        let mut class_edges = BTreeMap::<(u8, u32), Vec<ClassId>>::new();
        let mut direct = BTreeMap::<ReplayOwner, Vec<TypeId>>::new();

        let seed_owner = ReplayOwner::TypeGroup(TypeGroupId(0));
        let cross_owner = ReplayOwner::TypeGroup(TypeGroupId(1));
        for index in 0..spine_len - 1 {
            semantic_edges
                .entry(spine(index))
                .or_default()
                .push(spine(index + 1));
        }
        for index in 0..spine_len {
            semantic_edges.entry(cross).or_default().push(spine(index));
        }
        direct.insert(seed_owner, vec![TypeId(spine(0).1)]);
        direct.insert(cross_owner, vec![TypeId(cross.1)]);

        let classes = (0..siblings)
            .map(|index| ClassId(100 + u32::try_from(index).expect("class id fits u32")))
            .collect::<Vec<_>>();
        let sibling_owners = (0..siblings)
            .map(|index| {
                ReplayOwner::TypeGroup(TypeGroupId(
                    2 + u32::try_from(index).expect("owner id fits u32"),
                ))
            })
            .collect::<Vec<_>>();
        for index in 0..siblings {
            semantic_edges
                .entry(spine(spine_len - 1))
                .or_default()
                .push(sibling(index));
            semantic_edges
                .entry(sibling_root(index))
                .or_default()
                .push(sibling(index));
            direct.insert(sibling_owners[index], vec![TypeId(sibling_root(index).1)]);
            class_edges
                .entry(sibling(index))
                .or_default()
                .push(classes[index]);
        }

        let trace = ReplayDependencyTrace::default();
        if demand_first_sibling {
            // Only the first sibling owner's edge is authenticated; every other
            // emitted pair stays a coverage miss, which the caller counts.
            trace.enter(sibling_owners[0]);
            trace.demand(ReplayOwner::Class(classes[0]));
            trace.leave(sibling_owners[0]);
        }

        let mut work = TerminalClassDependencyValidationWork::default();
        require_terminal_class_dependency_closure(
            &trace,
            &semantic_edges,
            &class_edges,
            direct,
            Some(&mut work),
        );

        let error = trace
            .finish(
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                0,
            )
            .expect_err("the probe authenticates at most one emitted pair");
        let ReplayIndexGenerationError::TypedReferenceCoverage { count, .. } = error else {
            panic!("terminal closure must report its unauthenticated pairs: {error:?}");
        };
        (count, work)
    }

    /// Input + output size of the shared-union-spine shape: semantic nodes and
    /// edges, class edges, owner roots, and the emitted owner × class pairs.
    fn shared_union_spine_input_and_output(spine_len: usize, siblings: usize) -> usize {
        let semantic_nodes = spine_len + 1 + 2 * siblings;
        let semantic_edges = (spine_len - 1) + spine_len + 2 * siblings;
        let class_edges = siblings;
        let owner_roots = 2 + siblings;
        let output_pairs = 3 * siblings;
        semantic_nodes + semantic_edges + class_edges + owner_roots + output_pairs
    }

    #[test]
    fn terminal_class_validator_shared_union_spine_stays_complete() {
        const BASE: usize = 256;

        // Completeness: each class must still summarize to exactly its three
        // owners {seed, cross, own}, and an authenticated pair must be seen.
        let (base_misses, base_work) = shared_union_spine_probe(BASE, BASE, false);
        let (authenticated_misses, _) = shared_union_spine_probe(BASE, BASE, true);
        assert_eq!(
            base_misses,
            u64::try_from(3 * BASE).expect("pair count fits u64"),
            "each sibling class must summarize to exactly its three owners"
        );
        assert_eq!(
            authenticated_misses,
            base_misses - 1,
            "authenticating one owner-class edge must retire exactly one pair"
        );

        // The graph-build / SCC / Kahn counters stay linear on this shape — none
        // of them observes the owner-expression traversal, which the companion
        // `..._summary_is_output_sensitive` spec pins separately.
        let input_and_output = shared_union_spine_input_and_output(BASE, BASE);
        let counted = base_work
            .owner_root_summaries
            .saturating_add(base_work.semantic_node_visits)
            .saturating_add(base_work.semantic_edge_probes)
            .saturating_add(base_work.class_edge_probes)
            .saturating_add(base_work.class_summary_items);
        assert!(
            counted <= 6 * u64::try_from(input_and_output).expect("input fits u64"),
            "the committed counters are blind to the owner-expression traversal: \
             input+output={input_and_output}, work={base_work:?}"
        );
    }

    #[test]
    fn terminal_class_validator_shared_union_spine_summary_is_output_sensitive() {
        const BASE: usize = 256;
        const SCALED: usize = 1_024;

        let (base_misses, base_work) = shared_union_spine_probe(BASE, BASE, false);
        let (scaled_misses, scaled_work) = shared_union_spine_probe(SCALED, SCALED, false);
        assert_eq!(
            base_misses,
            u64::try_from(3 * BASE).expect("pair count fits u64"),
            "each sibling class must summarize to exactly its three owners"
        );
        assert_eq!(
            scaled_misses,
            u64::try_from(3 * SCALED).expect("pair count fits u64"),
            "each sibling class must summarize to exactly its three owners"
        );

        // Two sizes, so a super-linear summary is visible as a ratio and not
        // just as a budget overrun on one arbitrary shape.
        let base_budget = 6 * u64::try_from(shared_union_spine_input_and_output(BASE, BASE))
            .expect("validation input and output fit u64");
        let scaled_budget = 6 * u64::try_from(shared_union_spine_input_and_output(SCALED, SCALED))
            .expect("validation input and output fit u64");
        let base_visits = base_work.owner_expression_visits;
        let scaled_visits = scaled_work.owner_expression_visits;
        assert!(
            base_visits <= base_budget && scaled_visits <= scaled_budget,
            "owner-expression summarization must stay O(inputs + owners), not owners * spine \
             length: {BASE}x{BASE} visited {base_visits} (budget {base_budget}), \
             {SCALED}x{SCALED} visited {scaled_visits} (budget {scaled_budget})"
        );
    }

    #[test]
    fn declared_recipe_domain_uses_independent_base_and_local_ends() {
        const DECLARED_RECIPE_DOMAIN: u8 = 10;
        const TYPE_DOMAIN: u8 = 1;

        let state = compile_reservation_fixture(
            "declared-recipe-domain-ends.d.ts",
            "interface RecipeOwner { values: string[]; }",
        );
        let base = owned_state_identity_ends(&state);
        let recipe_end = state.interner.store().snapshot_declared_recipes().count();
        assert!(recipe_end > 0, "fixture must plan declaration recipes");

        let mut summary = OwnedBaseReferenceSummary::default();
        classify_live_reference(
            &mut summary,
            &base,
            DECLARED_RECIPE_DOMAIN,
            0,
            TYPE_DOMAIN,
            u32::try_from(base.store).expect("store end fits u32"),
        );
        classify_live_reference(
            &mut summary,
            &base,
            DECLARED_RECIPE_DOMAIN,
            u32::try_from(recipe_end).expect("recipe end fits u32"),
            TYPE_DOMAIN,
            0,
        );

        assert_eq!(
            summary,
            OwnedBaseReferenceSummary {
                base_to_delta: 1,
                delta_to_base: 1,
                delta_to_delta: 0,
            },
            "recipe rows must be classified against their own frozen prefix"
        );
    }

    #[test]
    fn store_prefix_digest_authenticates_unreferenced_planned_recipes() {
        let mut interner = Interner::with_intrinsics();
        let prefix_len = interner.store().len();
        let recipe_prefix_len = interner.store().snapshot_declared_recipes().count();
        let before = store_prefix_digest(interner.store(), prefix_len, recipe_prefix_len)
            .expect("initial prefix digest");
        let string = interner.well_known().string;
        interner.intern_declared_recipe(crate::types::repr::DeclaredRecipeNode::Type(string));
        let planned_recipe_prefix_len = interner.store().snapshot_declared_recipes().count();
        let after = store_prefix_digest(interner.store(), prefix_len, planned_recipe_prefix_len)
            .expect("prefix digest after recipe planning");

        assert_ne!(
            after, before,
            "an unreferenced recipe row is still authenticated prefix state"
        );
    }

    #[test]
    fn replay_index_records_empty_and_constructor_only_parent_edges() {
        let state = compile_reservation_fixture(
            "replay-class-parent-edges.d.ts",
            r#"
                declare class EmptyBase {}
                declare class EmptyChild extends EmptyBase {}
                declare class ConstructorBase { constructor(value: string); }
                declare class ConstructorChild extends ConstructorBase {
                    constructor(value: string);
                }
            "#,
        );
        let class = |name: &str| {
            let group = state
                .binder
                .type_groups
                .iter()
                .find(|group| group.name == name)
                .unwrap_or_else(|| panic!("missing class group {name}"));
            match state.published_types.groups().get(group.id) {
                Some(PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
                    surface: PublishedTypeGroupSurface::Class(class),
                    ..
                })) => *class,
                terminal => panic!("{name} did not publish a class: {terminal:?}"),
            }
        };
        let replay = state
            .replay_index()
            .expect("source compiler retains its replay index");

        for (parent, child) in [
            (class("EmptyBase"), class("EmptyChild")),
            (class("ConstructorBase"), class("ConstructorChild")),
        ] {
            assert!(replay.reverse_edges.contains(&ReplayReverseEdge {
                dependency: ReplayOwner::Class(parent),
                consumer: ReplayOwner::Class(child),
            }));
        }
    }

    fn root_candidate(
        slots: impl IntoIterator<Item = SourceBindingSlot>,
    ) -> crate::binder::declaration::SourceGlobalBindingCandidate {
        crate::binder::declaration::SourceGlobalBindingCandidate {
            slots: slots.into_iter().collect(),
            global_object_contributor: false,
        }
    }

    #[test]
    fn replay_root_census_rejects_a_missing_binder_root() {
        let candidates = BTreeMap::from([(
            "PresentOnlyInSource".to_owned(),
            root_candidate([SourceBindingSlot::Value]),
        )]);
        let error = validate_root_census(&candidates, &[], false).unwrap_err();
        assert_eq!(
            error,
            ReplayIndexGenerationError::InvalidRootSlot("PresentOnlyInSource".to_owned())
        );
    }

    #[test]
    fn replay_root_census_rejects_an_inexact_slot_set() {
        let candidates = BTreeMap::from([(
            "Merged".to_owned(),
            root_candidate([SourceBindingSlot::Value, SourceBindingSlot::Type]),
        )]);
        let rows = [RootNameRow {
            name: "Merged".to_owned(),
            value: Some(ValueStorageId(0)),
            ty: None,
            namespace: None,
        }];
        let error = validate_root_census(&candidates, &rows, false).unwrap_err();
        assert_eq!(
            error,
            ReplayIndexGenerationError::InvalidRootSlot("Merged".to_owned())
        );
    }

    #[test]
    fn replay_root_census_requires_explicit_global_this_to_publish() {
        let error = validate_root_census(&BTreeMap::new(), &[], true).unwrap_err();
        assert_eq!(
            error,
            ReplayIndexGenerationError::InvalidRootSlot("globalThis".to_owned())
        );
    }

    #[test]
    fn instantiated_namespace_expectation_does_not_depend_on_storage_or_root_row() {
        let mut candidate = root_candidate([SourceBindingSlot::Namespace]);
        apply_merge_root_semantics(
            &mut candidate,
            BTreeSet::from([SourceBindingSlot::Namespace]),
            false,
            true,
        );
        assert_eq!(
            candidate.slots,
            BTreeSet::from([SourceBindingSlot::Value, SourceBindingSlot::Namespace])
        );
        assert!(candidate.global_object_contributor);

        let candidates = BTreeMap::from([("RuntimeNamespace".to_owned(), candidate)]);
        let rows = [RootNameRow {
            name: "RuntimeNamespace".to_owned(),
            value: None,
            ty: None,
            namespace: Some(NamespaceId(0)),
        }];
        assert_eq!(
            validate_root_census(&candidates, &rows, false).unwrap_err(),
            ReplayIndexGenerationError::InvalidRootSlot("RuntimeNamespace".to_owned())
        );
    }

    #[test]
    fn replay_root_normalization_covers_pure_and_mixed_namespace_semantics() {
        let source = r#"
            declare namespace PureType { interface Member {} }
            declare namespace PureRuntime { const value: number; }
            interface InterfaceMerge {}
            declare namespace InterfaceMerge { interface Member {} }
            declare class ClassMerge {}
            declare namespace ClassMerge { interface Member {} }
            declare function FunctionMerge(): void;
            declare namespace FunctionMerge { interface Member {} }
        "#;
        let state = compile_reservation_fixture("root-normalization.d.ts", source);
        let replay = state
            .replay_index()
            .expect("source compiler retains its replay index");
        let root = |name: &str| {
            replay
                .root_slots
                .iter()
                .find(|root| root.name == name)
                .unwrap_or_else(|| panic!("missing replay root {name}"))
        };
        let slots = |name: &str| {
            let root = root(name);
            (
                root.value.is_some(),
                root.ty.is_some(),
                root.namespace.is_some(),
                root.global_object_contributor,
            )
        };
        assert_eq!(slots("PureType"), (false, false, true, false));
        assert_eq!(slots("PureRuntime"), (true, false, true, true));
        assert_eq!(slots("InterfaceMerge"), (false, true, true, false));
        assert_eq!(slots("ClassMerge"), (true, true, true, false));
        assert_eq!(slots("FunctionMerge"), (true, false, true, true));

        let binding_span = |name: &str| {
            let start = u32::try_from(source.find(name).expect("fixture binding")).unwrap();
            Span::new(start, start + u32::try_from(name.len()).unwrap())
        };
        let has_global_site = |span: Span| {
            replay
                .owner_sites
                .iter()
                .any(|site| site.owner == ReplayOwner::GlobalObject && site.span == span)
        };
        assert!(!has_global_site(binding_span("PureType")));
        assert!(has_global_site(binding_span("PureRuntime")));
    }

    #[test]
    fn object_alias_identity_ignores_member_source_order() {
        let state = compile_reservation_fixture(
            "object-alias-member-order.d.ts",
            r#"
                type OrderedFirst = { alpha: string; beta: number };
                type OrderedSecond = { beta: number; alpha: string };
            "#,
        );

        assert_eq!(
            published_template_type(&state, "OrderedFirst"),
            published_template_type(&state, "OrderedSecond"),
            "member source order is not structural identity"
        );
        assert_strict_interner_snapshot(&state, "object alias member order");
    }

    #[test]
    fn object_alias_identity_keeps_identity_bearing_metadata() {
        let state = compile_reservation_fixture(
            "object-alias-metadata.d.ts",
            r#"
                type OptionalValue = { value?: number };
                type RequiredValue = { value: number };
                type ReadonlyValue = { readonly value: number };
                type MutableValue = { value: number };
                type StringIndexed = { [key: string]: number };
                type NumberIndexed = { [key: number]: number };
                type Callable = { (input: string): boolean };
                type Constructable = { new (input: string): { value: boolean } };
            "#,
        );

        for (left, right) in [
            ("OptionalValue", "RequiredValue"),
            ("ReadonlyValue", "MutableValue"),
            ("StringIndexed", "NumberIndexed"),
            ("Callable", "Constructable"),
        ] {
            assert_ne!(
                published_template_type(&state, left),
                published_template_type(&state, right),
                "{left} and {right} differ in identity-bearing metadata"
            );
        }
        assert_strict_interner_snapshot(&state, "object alias identity metadata");
    }

    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "large object-alias scaling is a release-only gate"
    )]
    fn acyclic_object_alias_canonicalization_scales_linearly() {
        fn compile_scale(count: usize) -> (Duration, usize, usize) {
            let mut source = String::new();
            for index in 0..count {
                use std::fmt::Write;
                writeln!(
                    source,
                    "type Scale{index:05} = {{ value: number; run(input: string): boolean }};"
                )
                .expect("write scale fixture");
            }
            let (run, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(0),
                name: "object-alias-scale.d.ts",
                source: &source,
            }])
            .expect("object-alias scale fixture compiles");
            let identities = state
                .binder
                .type_groups
                .iter()
                .filter(|group| group.name.starts_with("Scale"))
                .map(|group| match state.published_types.groups().get(group.id) {
                    Some(PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
                        surface: PublishedTypeGroupSurface::Template(ty),
                        ..
                    })) => *ty,
                    terminal => panic!("scale alias did not publish: {terminal:?}"),
                })
                .collect::<rustc_hash::FxHashSet<_>>();
            assert_strict_interner_snapshot(&state, "object alias scale");
            eprintln!(
                "typokat-object-alias-scale-v1 aliases={count} store_rows={} reserve_fill_us={} unique_published={}",
                state.interner.store().len(),
                run.phase_timings.reserve_fill.as_micros(),
                identities.len(),
            );
            (
                run.phase_timings.reserve_fill,
                state.interner.store().len(),
                identities.len(),
            )
        }

        let (small_time, small_rows, small_unique) = compile_scale(1_000);
        let (large_time, large_rows, large_unique) = compile_scale(10_000);
        assert_eq!((small_unique, large_unique), (1, 1));
        assert!(large_rows < small_rows.saturating_mul(12));
        assert!(
            large_time <= small_time.saturating_mul(20),
            "10x aliases must not approach quadratic reserve/fill time: {small_time:?} -> {large_time:?}"
        );
    }

    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "full-profile reservation closure is release-only"
    )]
    fn exact_profile_interner_has_no_pending_reservations() {
        let profile = ExactLibraryProfile::load_packaged().expect("packaged full-library profile");
        let (_, state) = compile_owned_injected_profile(&injected_packaged_sources(&profile))
            .expect("source-compiled full-library profile");
        state
            .interner
            .encode_snapshot_bytes_for_test()
            .expect("full profile must close every reserved type before snapshot");
    }

    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "full-profile replay-index generation is release-only"
    )]
    fn exact_profile_replay_index_is_complete_and_deterministic() {
        let profile = ExactLibraryProfile::load_packaged().expect("packaged full-library profile");
        let injected = injected_packaged_sources(&profile);
        let started = Instant::now();
        let (_, first_state) =
            compile_owned_injected_profile(&injected).expect("first exact replay index generation");
        let first_elapsed = started.elapsed();
        let started = Instant::now();
        let (_, second_state) = compile_owned_injected_profile(&injected)
            .expect("second exact replay index generation");
        let second_elapsed = started.elapsed();
        let first = first_state
            .replay_index()
            .expect("source compiler retains its replay index");
        let second = second_state
            .replay_index()
            .expect("source compiler retains its replay index");
        assert_eq!(
            first.canonical_manifest_len(),
            second.canonical_manifest_len()
        );
        assert_eq!(
            first.canonical_manifest_sha256,
            second.canonical_manifest_sha256
        );
        assert_eq!(first.unowned_demand_count, 0);
        assert_eq!(first.invalid_owner_site_count, 0);
        assert_eq!(first.noncanonical_edge_count, 0);
        assert_eq!(first.typed_reference_coverage_misses, 0);
        let digest = first
            .canonical_manifest_sha256
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        eprintln!(
            "replay-index owners={} roots={} sites={} edges={} root_edges={} sccs={} statements={} baselines={} bytes={} sha256={} first={:?} second={:?}",
            first.owner_partition.len(),
            first.root_slots.len(),
            first.owner_sites.len(),
            first.reverse_edges.len(),
            first.root_slot_consumers.len(),
            first.scc_membership.len(),
            first.statement_owners.len(),
            first.baseline_records.len(),
            first.canonical_manifest_len(),
            digest,
            first_elapsed,
            second_elapsed,
        );
    }

    #[test]
    fn focused_profile_selects_complete_native_bridge_identities() {
        let run = run_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "native-bridges.d.ts",
            source: r#"
                interface Array<T> { mapResult: T; }
                interface ReadonlyArray<T> { mapResult: T; }
                interface String { toUpperCaseResult: string; }
                interface Number { toFixedResult: string; }
                interface Boolean { valueOfResult: boolean; }
                interface RegExp { testResult: boolean; }
                interface Object { toStringResult: string; }
                interface Function { legacyCallResult: string; }
                interface CallableFunction extends Function { callResult: number; }
            "#,
        }])
        .expect("focused source-compiled library profile");
        let identities = run.semantic_identities();
        assert!(identities.all_ready());
        assert_eq!(
            identities.callable_function_group(),
            run.global_type_probe("CallableFunction")
                .map(|probe| probe.identity)
        );
        assert_ne!(
            identities.callable_function_group(),
            run.global_type_probe("Function")
                .map(|probe| probe.identity)
        );
        let expected_arities = [1, 1, 0, 0, 0, 0, 0, 0];
        for (terminal, expected_arity) in identities.terminals().into_iter().zip(expected_arities) {
            let super::super::library_identities::LibraryIdentityTerminal::Ready(identity) =
                terminal
            else {
                panic!("focused native bridge identity must be ready")
            };
            assert_eq!(identity.parameters.len(), expected_arity);
        }
    }

    #[test]
    fn owned_runtime_snapshot_parts_restore_a_consumable_base() {
        let parts = compile_semantic_snapshot_parts();
        assert_eq!(parts.source_file_count, 1);
        assert!(parts.semantic_identities.is_some());
        assert!(!parts.runtime.class_application_parameters.is_empty());
        assert!(!parts.runtime.class_new_metadata.is_empty());
        assert!(!parts.runtime.class_value_bindings.is_empty());
        assert!(!parts.runtime.namespace_terminals.is_empty());
        assert!(!parts.runtime.named_function_symbols.is_empty());
        let restored =
            OwnedLibraryRuntimeState::from_snapshot_parts(parts).expect("restore snapshot parts");
        let user = check_caller_certified_collision_free_source_with_owned_library(
            restored,
            r#"
                declare const witness: SnapshotWitness;
                const witnessValue: number = witness.value;
                declare const nativeValues: number[];
                const nativeArrayItem: number = nativeValues.item;
                const nativeStringMarker: string = "value".stringMarker;
                const instance = new SnapshotClass<number>(1);
                const classValue: number = instance.value;
                const inheritedValue: string = instance.base;
                const base = new SnapshotBase();
                const directBaseValue: string = base.base;
                const classTag: string = SnapshotClass.tag;
                const enabled: boolean = SnapshotSpace.enabled;
                const called: string = snapshotNamed(1);
                const functionVersion: number = snapshotNamed.version;
            "#,
        )
        .expect("consume restored base");

        assert!(user.result.diagnostics.is_empty());
        assert!(user.result.incomplete.is_empty());
        assert_eq!(user.witness.base_max_source_key, 1);
    }

    #[test]
    fn owned_runtime_snapshot_restores_native_bridge_error_behavior() {
        let restored =
            OwnedLibraryRuntimeState::from_snapshot_parts(compile_semantic_snapshot_parts())
                .expect("restore snapshot parts");
        let user = check_caller_certified_collision_free_source_with_owned_library(
            restored,
            r#"
                declare const nativeValues: number[];
                const wrongNativeItem: string = nativeValues.item;
            "#,
        )
        .expect("consume restored native bridge");

        assert_eq!(user.result.diagnostics.len(), 1);
        assert_eq!(
            user.result.diagnostics[0].code,
            crate::diagnostics::DiagnosticCode::TK2322
        );
        assert!(user.result.incomplete.is_empty());
    }

    #[test]
    fn owned_runtime_snapshot_rejects_empty_and_gapped_source_prefixes() {
        let mut empty = compile_semantic_snapshot_parts();
        replace_module_sources(&mut empty, Default::default());
        assert_snapshot_restore_error(empty, "snapshot binder has no retained source ownership");

        let mut gapped = compile_semantic_snapshot_parts();
        let mut sources = gapped
            .binder
            .snapshot_module_sources()
            .iter()
            .map(|(&scope, &source)| (scope, source))
            .collect::<rustc_hash::FxHashMap<_, _>>();
        let library_scope = sources
            .iter()
            .find_map(|(scope, source)| (source.0 == 1).then_some(*scope))
            .expect("library source owner");
        sources.insert(library_scope, exact_key(2));
        replace_module_sources(&mut gapped, sources);
        assert_snapshot_restore_error(
            gapped,
            "snapshot binder source keys are not the contiguous prelude/library prefix",
        );
    }

    #[test]
    fn owned_runtime_snapshot_rejects_group_and_counter_collisions() {
        let mut groups = compile_semantic_snapshot_parts();
        groups.published_types.groups.pop();
        assert_snapshot_restore_error(
            groups,
            "snapshot published type-group count does not match the binder",
        );

        let mut type_parameters = compile_semantic_snapshot_parts();
        type_parameters.next_type_param = 0;
        assert_snapshot_restore_error(
            type_parameters,
            "snapshot type parameter id collides with the next parameter counter",
        );

        let mut classes = compile_semantic_snapshot_parts();
        classes.next_class_id = 0;
        assert_snapshot_restore_error(
            classes,
            "snapshot class id collides with the next class counter",
        );

        let mut renamed = compile_semantic_snapshot_parts();
        let template_group = renamed
            .published_types
            .groups
            .iter_mut()
            .find_map(|terminal| match terminal {
                PublishedTypeGroupTerminal::Ready(group)
                    if group.name == "SnapshotWitness"
                        && matches!(group.surface, PublishedTypeGroupSurface::Template(_)) =>
                {
                    Some(group)
                }
                PublishedTypeGroupTerminal::Ready(_)
                | PublishedTypeGroupTerminal::Unavailable(_) => None,
            })
            .expect("ready template group");
        template_group.name = "RenamedSnapshotWitness".to_owned();
        assert_snapshot_restore_error(
            renamed,
            "snapshot published type-group name does not match the binder",
        );
    }

    #[test]
    fn owned_runtime_snapshot_rejects_missing_required_class_rows() {
        let mut new_metadata = compile_semantic_snapshot_parts();
        let removed_class = new_metadata
            .runtime
            .class_new_metadata
            .pop()
            .expect("new metadata row")
            .0;
        assert!(new_metadata
            .runtime
            .class_value_bindings
            .iter()
            .any(|(_, binding)| binding.class_id == removed_class));
        assert_snapshot_restore_error(
            new_metadata,
            "snapshot new metadata does not exactly cover published classes",
        );

        let mut applications = compile_semantic_snapshot_parts();
        applications
            .runtime
            .class_application_parameters
            .pop()
            .expect("class application row");
        assert_snapshot_restore_error(
            applications,
            "snapshot class application metadata does not exactly cover published classes",
        );

        let mut names = compile_semantic_snapshot_parts();
        names.runtime.class_names.pop().expect("class name row");
        assert_snapshot_restore_error(
            names,
            "snapshot class names do not exactly match published class groups",
        );

        let mut bindings = compile_semantic_snapshot_parts();
        bindings
            .runtime
            .class_value_bindings
            .pop()
            .expect("class value binding row");
        assert_snapshot_restore_error(
            bindings,
            "snapshot class value bindings do not exactly cover published classes",
        );

        let mut parent_chain = compile_semantic_snapshot_parts();
        let inherited = parent_chain
            .runtime
            .class_new_metadata
            .iter()
            .find_map(|(class, metadata)| {
                (*class != metadata.ctor_declaring_class).then_some(*class)
            })
            .expect("class with inherited constructor owner");
        let parent_row = parent_chain
            .runtime
            .class_parents
            .iter()
            .position(|(class, _)| *class == inherited)
            .expect("inherited class parent row");
        parent_chain.runtime.class_parents.remove(parent_row);
        assert_snapshot_restore_error(
            parent_chain,
            "snapshot constructor owner is not on the class parent chain",
        );

        let mut wrong_name = compile_semantic_snapshot_parts();
        wrong_name.runtime.class_names[0].1 = "DifferentClass".to_owned();
        assert_snapshot_restore_error(
            wrong_name,
            "snapshot class names do not exactly match published class groups",
        );

        let mut generic_bit = compile_semantic_snapshot_parts();
        let generic_class = generic_bit
            .runtime
            .class_application_parameters
            .iter()
            .find_map(|(class, parameters)| (!parameters.is_empty()).then_some(*class))
            .expect("generic class application");
        let binding = generic_bit
            .runtime
            .class_value_bindings
            .iter_mut()
            .find_map(|(_, binding)| (binding.class_id == generic_class).then_some(binding))
            .expect("generic class value binding");
        binding.has_header_type_params = !binding.has_header_type_params;
        assert_snapshot_restore_error(
            generic_bit,
            "snapshot class value binding generic bit does not match application metadata",
        );
    }

    #[test]
    fn owned_runtime_snapshot_rejects_dangling_semantic_and_type_references() {
        let mut semantic = compile_semantic_snapshot_parts();
        let LibraryIdentityTerminal::Ready(identity) = &mut semantic
            .semantic_identities
            .as_mut()
            .expect("semantic identities")[0]
        else {
            panic!("Array identity is ready")
        };
        identity.group = TypeGroupId(u32::MAX);
        assert_snapshot_restore_error(
            semantic,
            "snapshot semantic identity refers to an unpublished type group",
        );

        let mut types = compile_semantic_snapshot_parts();
        let ready = types
            .runtime
            .namespace_terminals
            .iter_mut()
            .find_map(|row| match &mut row.terminal {
                FrozenNamespaceValueTerminalSnapshot::Ready { ty, .. } => Some(ty),
                FrozenNamespaceValueTerminalSnapshot::Unavailable(_) => None,
            })
            .expect("ready namespace terminal");
        *ready = TypeId(u32::MAX);
        assert_snapshot_restore_error(
            types,
            "snapshot namespace terminal has an invalid ready reference",
        );
    }

    #[test]
    fn owned_runtime_snapshot_rejects_dangling_runtime_binder_references() {
        let mut values = compile_semantic_snapshot_parts();
        values
            .runtime
            .class_value_bindings
            .first_mut()
            .expect("class value binding")
            .0 = ValueStorageId(u32::MAX);
        assert_snapshot_restore_error(
            values,
            "snapshot class value binding has an invalid reference",
        );

        let mut symbols = compile_semantic_snapshot_parts();
        symbols.runtime.named_function_symbols[0] = SymbolId(u32::MAX);
        assert_snapshot_restore_error(
            symbols,
            "snapshot named-function metadata refers to a non-function symbol",
        );

        let mut namespaces = compile_semantic_snapshot_parts();
        namespaces.runtime.namespace_terminals[0].namespace =
            crate::binder::namespace::NamespaceId(u32::MAX);
        assert_snapshot_restore_error(
            namespaces,
            "snapshot namespace terminal refers to an unknown namespace",
        );
    }

    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "full-profile semantic selection is release-only"
    )]
    fn exact_profile_selects_complete_native_bridge_identities() {
        let profile = ExactLibraryProfile::load_packaged().expect("packaged full-library profile");
        let run = run_injected_profile(&injected_packaged_sources(&profile))
            .expect("source-compiled full-library profile");
        let identities = run.semantic_identities();
        assert!(identities.all_ready());
        assert_eq!(
            identities.callable_function_group(),
            run.global_type_probe("CallableFunction")
                .map(|probe| probe.identity)
        );
        assert_ne!(
            identities.callable_function_group(),
            run.global_type_probe("Function")
                .map(|probe| probe.identity)
        );
        let expected_arities = [1, 1, 0, 0, 0, 0, 0, 0];
        for (terminal, expected_arity) in identities.terminals().into_iter().zip(expected_arities) {
            let super::super::library_identities::LibraryIdentityTerminal::Ready(identity) =
                terminal
            else {
                panic!("exact profile native bridge identity must be ready")
            };
            assert_eq!(identity.parameters.len(), expected_arity);
            assert_ne!(identity.template, TypeId(0));
        }
    }

    #[test]
    fn phase_timings_are_real_nonoverlapping_measurements() {
        let started = Instant::now();
        let run = run_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "timing.d.ts",
            source: "interface TimingWitness { value: number; }",
        }])
        .expect("focused timing profile");
        let external = started.elapsed();
        assert!(run.phase_timings.measured_total() <= external);
        assert_eq!(run.phase_counts.parse_units, 1);
        assert_eq!(run.phase_counts.bind_units, 1);
        assert_eq!(run.phase_counts.statement_check_units, 1);
    }

    #[test]
    fn release_probe_lines_have_the_exact_v1_contract() {
        let timings = LibraryPhaseTimings {
            parse: Duration::from_micros(2),
            bind: Duration::from_micros(3),
            reserve_fill: Duration::from_micros(5),
            publication_validation: Duration::from_micros(7),
            statement_check: Duration::from_micros(11),
        };
        assert_eq!(
            release_phase_line(
                4,
                Duration::from_micros(1),
                &timings,
                Duration::from_micros(31),
            ),
            "typokat-wu0b-phase-v1 process=4 registry_validation_us=1 parse_us=2 bind_us=3 reserve_fill_us=5 publication_validation_us=7 statement_check_us=11 total_us=31"
        );
        assert_eq!(
            ReleaseOutcomeLine {
                process: 4,
                file_count: 82,
                reserved_records: 13,
                filled_records: 13,
                publication_validations: 9,
                library_diagnostics: 2,
                library_incompletes: 3,
                tiny_parse_errors: 0,
                tiny_diagnostics: 0,
                tiny_incompletes: 0,
            }
            .render(),
            "typokat-wu0b-outcome-v1 process=4 file_count=82 reserved_records=13 filled_records=13 publication_validations=9 library_diagnostics=2 library_incompletes=3 tiny_parse_errors=0 tiny_diagnostics=0 tiny_incompletes=0"
        );
    }

    #[test]
    #[ignore = "release-only cold-process library measurement"]
    fn library_release_probe_once() {
        let process = std::env::var("TYPOKAT_WU0B_PROCESS")
            .expect("TYPOKAT_WU0B_PROCESS must identify release process 1..5")
            .parse::<usize>()
            .expect("TYPOKAT_WU0B_PROCESS must be an integer in 1..5");
        assert!(
            (1..=5).contains(&process),
            "TYPOKAT_WU0B_PROCESS must be in 1..5"
        );

        let total_started = Instant::now();
        let registry_started = Instant::now();
        let profile =
            ExactLibraryProfile::load_packaged().expect("packaged library registry validation");
        let registry_validation = registry_started.elapsed();
        let injected = injected_packaged_sources(&profile);
        let run = run_injected_profile(&injected).expect("exact library profile execution");

        assert_eq!(run.phase_counts.parse_units, 82);
        assert_eq!(run.phase_counts.bind_units, 82);
        assert_eq!(run.phase_counts.statement_check_units, 82);
        assert!(run.phase_counts.reserved_records > 0);
        assert_eq!(
            run.phase_counts.reserved_records,
            run.phase_counts.filled_records
        );
        assert!(run.phase_counts.publication_validations > 0);
        let expected = (0..82)
            .map(LibraryFileOrdinal::new)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            run.reserved_file_ordinals
                .iter()
                .copied()
                .collect::<BTreeSet<_>>(),
            expected
        );
        assert!(run
            .library_records
            .iter()
            .all(|(key, _)| expected.contains(&key.file_ordinal)));
        assert!(run
            .reporting_receipts
            .iter()
            .all(|receipt| expected.contains(&receipt.file_ordinal)));

        let tiny = check_source(TINY_SOURCE);
        assert!(tiny.parse_errors.is_empty(), "{:?}", tiny.parse_errors);
        assert!(tiny.diagnostics.is_empty(), "{:?}", tiny.diagnostics);
        assert!(tiny.incomplete.is_empty(), "{:?}", tiny.incomplete);
        let library_diagnostics = run
            .library_records
            .iter()
            .filter(|(_, record)| matches!(record, CheckerRecord::Diagnostic(_)))
            .count();
        let library_incompletes = run.library_records.len() - library_diagnostics;
        let total = total_started.elapsed();
        assert!(
            registry_validation + run.phase_timings.measured_total() <= total,
            "external total must cover every measured phase"
        );

        println!(
            "{}",
            release_phase_line(process, registry_validation, &run.phase_timings, total)
        );
        println!(
            "{}",
            ReleaseOutcomeLine {
                process,
                file_count: injected.len(),
                reserved_records: run.phase_counts.reserved_records,
                filled_records: run.phase_counts.filled_records,
                publication_validations: run.phase_counts.publication_validations,
                library_diagnostics,
                library_incompletes,
                tiny_parse_errors: tiny.parse_errors.len(),
                tiny_diagnostics: tiny.diagnostics.len(),
                tiny_incompletes: tiny.incomplete.len(),
            }
            .render()
        );
    }

    #[test]
    fn recoverable_parser_diagnostic_fails_closed_before_profile_execution() {
        let file_ordinal = LibraryFileOrdinal::new(3);
        let source = "declare namespace Broken { export = Broken; }";
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(!parsed.panicked, "witness must be recoverable");
        assert_eq!(parsed.diagnostics.len(), 1, "witness must diagnose once");
        assert_eq!(parsed.diagnostics[0].code.scope.as_deref(), Some("TS"));
        assert_eq!(parsed.diagnostics[0].code.number.as_deref(), Some("1063"));
        let result = run_injected_profile(&[InjectedLibrarySource {
            file_ordinal,
            name: "recoverable.ts",
            source,
        }]);
        let Err(InjectedProfileError::Parse {
            file_ordinal: actual,
            messages,
        }) = result
        else {
            panic!("recoverable parser diagnostics must abort the injected run");
        };
        assert_eq!(actual, file_ordinal);
        assert!(!messages.is_empty());
    }

    #[test]
    fn focused_shared_interface_identity_and_surface() {
        let run = run_injected_profile(&[
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(1),
                name: "first.d.ts",
                source: "interface Shared { first: number; }",
            },
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(2),
                name: "second.d.ts",
                source: "interface Shared { second: string; }",
            },
        ])
        .expect("focused injected profile");
        let shared = run.global_type_probe("Shared").expect("shared type probe");
        assert_eq!(shared.declaration_count, 2);
        assert_eq!(shared.member_names, ["first", "second"]);
        assert_eq!(run.phase_counts.parse_units, 2);
        assert_eq!(run.phase_counts.bind_units, 2);
        assert_eq!(run.phase_counts.statement_check_units, 2);
        assert_eq!(
            run.phase_counts.reserved_records,
            run.phase_counts.filled_records
        );
        assert_eq!(run.phase_counts.publication_validations, 1);
        assert_eq!(
            run.reserved_file_ordinals,
            [LibraryFileOrdinal::new(1), LibraryFileOrdinal::new(2)]
        );
        assert!(run.reporting_receipts.is_empty());
        assert!(run.library_records.is_empty());
    }

    #[test]
    fn focused_implementation_diagnostic_keeps_exact_library_key() {
        let file_ordinal = LibraryFileOrdinal::new(7);
        let run = run_injected_profile(&[InjectedLibrarySource {
            file_ordinal,
            name: "broken.ts",
            source: "const broken: number = 'wrong';",
        }])
        .expect("focused injected profile");
        assert_eq!(run.pass_source_units, [library_unit(file_ordinal)]);
        assert!(!run.lexical_source_units.is_empty());
        assert_eq!(run.library_records.len(), 1);
        assert_eq!(run.library_records[0].0.file_ordinal, file_ordinal);
        let CheckerRecord::Diagnostic(diagnostic) = &run.library_records[0].1 else {
            panic!("implementation mismatch must be a diagnostic");
        };
        assert_eq!(run.library_records[0].0.source_start, 23);
        assert_eq!(diagnostic.span.start, 6);
    }

    #[test]
    fn focused_function_namespace_and_module_private_probes() {
        let script = LibraryFileOrdinal::new(10);
        let module = LibraryFileOrdinal::new(11);
        let run = run_injected_profile(&[
            InjectedLibrarySource {
                file_ordinal: script,
                name: "function.d.ts",
                source: "declare function Merged(value: number): string; declare namespace Merged { export function member(value: string): number; } interface Shared { script: number; }",
            },
            InjectedLibrarySource {
                file_ordinal: module,
                name: "module.d.ts",
                source: "export {}; interface Private { local: boolean; } declare global { interface Shared { module: string; } }",
            },
        ])
        .expect("focused injected profile");

        let merged = run.global_value_probe("Merged").expect("merged value");
        assert_eq!(merged.declaration_count, 2);
        assert_eq!(merged.call_signature_count, 1, "{merged:?}");
        assert_eq!(merged.member_names, ["member"], "{merged:?}");
        assert_eq!(merged.callable_members.len(), 1);
        assert_eq!(merged.callable_members[0].signatures.len(), 1);
        assert!(run.global_type_probe("Private").is_none());
        assert!(run.module_type_probe(module, "Private").is_some());
        assert_eq!(
            run.global_type_probe("Shared")
                .expect("augmented global")
                .declaration_count,
            2
        );
    }

    #[test]
    fn focused_typed_ts1319_claim_is_reported_once_by_binder_consumer() {
        let file_ordinal = LibraryFileOrdinal::new(14);
        let run = run_injected_profile(&[InjectedLibrarySource {
            file_ordinal,
            name: "export-context.d.ts",
            source: "declare namespace Exported { export default function f(): void; }",
        }])
        .expect("typed TS1319 claim must transfer to binder reporting");
        let receipt = run
            .reporting_receipts
            .iter()
            .find(|receipt| receipt.family == LibraryReportingFamily::ExportContext)
            .expect("export-context receipt");
        assert_eq!(receipt.file_ordinal, file_ordinal);
        assert_eq!(receipt.observed_outcomes, 1);
        assert_eq!(receipt.emitted_records, 1);
        assert_eq!(run.library_records.len(), 1);
        let (key, record) = &run.library_records[0];
        assert_eq!(key.file_ordinal, file_ordinal);
        assert_eq!(key.source_start, 29);
        let CheckerRecord::Incomplete(incomplete) = record else {
            panic!("TK1319 reporting must remain an incomplete record");
        };
        assert_eq!(incomplete.id, "library/export-context/future-tk1319");
        assert_eq!(
            incomplete.context,
            "library export-context TK1319 reporting is deferred beyond snapshot feasibility"
        );
        assert_eq!(incomplete.span, Span::new(29, 63));
        assert_eq!(key.source_start, incomplete.span.start);
    }

    #[test]
    fn mixed_ts1319_and_ts1063_diagnostics_fail_closed() {
        let file_ordinal = LibraryFileOrdinal::new(15);
        let source =
            "declare namespace Mixed { export default function f(): void; export = Mixed; }";
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(!parsed.panicked, "witness must be recoverable");
        let codes = parsed
            .diagnostics
            .iter()
            .map(|diagnostic| {
                (
                    diagnostic.code.scope.as_deref(),
                    diagnostic.code.number.as_deref(),
                )
            })
            .collect::<Vec<_>>();
        assert!(codes.contains(&(Some("TS"), Some("1319"))), "{codes:?}");
        assert!(codes.contains(&(Some("TS"), Some("1063"))), "{codes:?}");
        let result = run_injected_profile(&[InjectedLibrarySource {
            file_ordinal,
            name: "mixed.ts",
            source,
        }]);
        assert!(matches!(
            result,
            Err(InjectedProfileError::Parse {
                file_ordinal: actual,
                ..
            }) if actual == file_ordinal
        ));
    }

    #[test]
    fn parser_export_claim_inventory_rejects_duplicates_and_unmatched_binder_claims() {
        let claim = ParserExportClaim {
            file_ordinal: LibraryFileOrdinal::new(16),
            span: Span::new(4, 12),
        };
        assert!(matches!(
            match_parser_export_claims(vec![claim, claim], vec![claim]),
            Err(InjectedProfileError::Parse { .. })
        ));
        assert!(matches!(
            match_parser_export_claims(Vec::new(), vec![claim]),
            Err(InjectedProfileError::Parse { .. })
        ));
    }

    #[test]
    fn focused_identical_callable_offsets_keep_exact_owners_and_signatures() {
        let first = LibraryFileOrdinal::new(34);
        let second = LibraryFileOrdinal::new(35);
        let sources = [
            InjectedLibrarySource {
                file_ordinal: first,
                name: "first.d.ts",
                source: "declare namespace OffsetCallable { export function alpha(value: number): string; }\ndeclare function OffsetCallable(): void;",
            },
            InjectedLibrarySource {
                file_ordinal: second,
                name: "second.d.ts",
                source: "declare namespace OffsetCallable { export function bravo(value: string): number; }",
            },
        ];
        let reversed = [sources[1], sources[0]];

        for run in [
            run_injected_profile(&sources).expect("forward injected profile"),
            run_injected_profile(&reversed).expect("reverse injected profile"),
        ] {
            let merged = run
                .global_value_probe("OffsetCallable")
                .expect("merged callable");
            let mut members = merged.callable_members.clone();
            members.sort_by(|left, right| left.name.cmp(&right.name));
            assert_eq!(members.len(), 2);
            assert_ne!(members[0].identity, members[1].identity);
            for (member, name, file, parameter, result) in [
                (&members[0], "alpha", first, "number", "string"),
                (&members[1], "bravo", second, "string", "number"),
            ] {
                assert_eq!(member.name, name);
                assert_eq!(member.source, library_unit(file));
                assert_eq!(member.reservation_source, library_unit(file));
                assert_eq!(member.source_start, 42);
                assert_eq!(member.call_signature_count, 1);
                assert_eq!(member.signatures.len(), 1);
                assert_eq!(member.signatures[0].parameter_types, [parameter]);
                assert_eq!(member.signatures[0].return_type, result);
            }
        }
    }

    #[test]
    fn focused_function_namespace_merges_are_input_order_independent() {
        let sources = [
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(30),
                name: "function.d.ts",
                source: "declare function FunctionFirst(value: number): string;",
            },
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(31),
                name: "function-namespace.d.ts",
                source: "declare namespace FunctionFirst { export const tag: number; }",
            },
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(32),
                name: "namespace.d.ts",
                source: "declare namespace NamespaceFirst { export const tag: string; }",
            },
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(33),
                name: "namespace-function.d.ts",
                source: "declare function NamespaceFirst(value: string): number;",
            },
        ];
        let reversed = [sources[3], sources[2], sources[1], sources[0]];
        for run in [
            run_injected_profile(&sources).expect("forward injected profile"),
            run_injected_profile(&reversed).expect("reverse injected profile"),
        ] {
            for name in ["FunctionFirst", "NamespaceFirst"] {
                let merged = run.global_value_probe(name).expect("merged value");
                assert_eq!(merged.declaration_count, 2);
                assert_eq!(merged.call_signature_count, 1);
                assert_eq!(merged.member_names, ["tag"]);
            }
            assert!(run.library_records.is_empty());
        }
    }

    #[test]
    fn focused_parse_clean_binder_reporting_families_have_real_receipts() {
        for (index, name, source, family) in [
            (
                10,
                "alias.d.ts",
                "declare namespace AliasOutput { interface Local {} export { A as B }; export { Local as A }; }",
                LibraryReportingFamily::LocalAmbientExportAliasFailure,
            ),
            (
                11,
                "placement.ts",
                "namespace Late { export const value = 1; } function Late(): void {}",
                LibraryReportingFamily::PlacementIssue,
            ),
            (
                12,
                "global.d.ts",
                "declare global { interface InvalidScriptGlobal {} }",
                LibraryReportingFamily::GlobalAugmentation,
            ),
            (
                13,
                "umd.d.ts",
                "export as namespace ScriptUmd;",
                LibraryReportingFamily::UmdExportContext,
            ),
            (
                15,
                "member.d.ts",
                "declare namespace MemberRoot { const value: number; }",
                LibraryReportingFamily::NamespaceMember,
            ),
            (
                16,
                "standalone.d.ts",
                "declare namespace StandaloneRoot { const value: number; }",
                LibraryReportingFamily::StandaloneNamespaceValueMember,
            ),
        ] {
            let file_ordinal = LibraryFileOrdinal::new(index);
            let run = run_injected_profile(&[InjectedLibrarySource {
                file_ordinal,
                name,
                source,
            }])
            .expect("binder reporting profile");
            let receipt = run
                .reporting_receipts
                .iter()
                .find(|receipt| receipt.family == family)
                .expect("family receipt");
            assert_eq!(receipt.file_ordinal, file_ordinal);
            assert_eq!(receipt.observed_outcomes, 1);
        }
    }

    const OWNED_MINI_LIBRARY: &str = r#"
        interface Object { toString(): string; }
        interface Function {}
        interface CallableFunction extends Function {
            call<T, A extends unknown[], R>(this: (this: T, ...args: A) => R, thisArg: T, ...args: A): R;
        }
        interface String { toUpperCase(): string; }
        interface Number { toFixed(fractionDigits?: number): string; }
        interface Boolean { valueOf(): boolean; }
        interface Array<T> {
            map<U>(callbackfn: (value: T) => U): U[];
            push(...items: T[]): number;
        }
        interface ReadonlyArray<T> { map<U>(callbackfn: (value: T) => U): U[]; }
        interface RegExp { test(value: string): boolean; }
        interface HTMLElement {}
        interface HTMLDivElement extends HTMLElement { align: string; }
        interface HTMLElementTagNameMap { div: HTMLDivElement; }
        interface ElementCreationOptions {}
        interface Document {
            createElement<K extends keyof HTMLElementTagNameMap>(
                tagName: K,
                options?: ElementCreationOptions,
            ): HTMLElementTagNameMap[K];
        }
        declare var document: Document;
        declare function increment(value: number): number;
        declare namespace increment { const identity: string; }
        declare class LibraryBase<T = string> {
            constructor(value: T);
            value: T;
        }
        declare namespace LibraryBase { const kind: string; }
        declare class PrivateLibraryClass { private constructor(); }
        declare namespace RuntimeBag { const answer: number; }
    "#;

    const OWNED_MINI_USER: &str = r#"
        class UserSuffix {}
        const mappedOk: string[] = [1].map(value => value.toFixed());
        const mappedBad: number[] = [1].map(value => value.toFixed());
        const readonlyValues: readonly number[] = [1, 2];
        const readonlyBad: number[] = readonlyValues.map(value => value.toFixed());
        readonlyValues.push(3);
        const upperBad: number = "x".toUpperCase();
        const fixedBad: number = (1).toFixed();
        const booleanBad: string = true.valueOf();
        const calledBad: number = ((value: string) => value).call(undefined, "x");
        const objectBad: number = ({ value: 1 }).toString();
        const regexpBad: string = /x/.test("x");
        const domOk: HTMLDivElement = document.createElement("div");
        const domBad: number = document.createElement("div");
        const libraryClass = new LibraryBase<string>("x");
        const libraryValue: string = libraryClass.value;
        const libraryKind: string = LibraryBase.kind;
        const runtimeAnswer: number = RuntimeBag.answer;
        const RuntimeAlias = RuntimeBag;
        const aliasAnswer: number = RuntimeAlias.answer;
        increment.notAFunctionMember;
    "#;

    #[test]
    fn owned_generic_promise_identity_preserves_resolve_argument() {
        let library = r#"
            type SnapshotAwaited<T> = T;
            interface SnapshotPromise<T> {
                then<Result = T>(
                    onfulfilled?: ((value: T) => Result) | null,
                ): SnapshotPromise<Result>;
            }
            interface SnapshotPromiseConstructor {
                resolve<T>(value: T): SnapshotPromise<SnapshotAwaited<T>>;
            }
            declare var SnapshotPromise: SnapshotPromiseConstructor;
        "#;
        let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "promise-identity.d.ts",
            source: library,
        }])
        .expect("focused Promise identity library compiles");
        let run = check_caller_certified_collision_free_source_with_owned_library(
            state,
            r#"
                declare const awaited: SnapshotAwaited<number>;
                const wrongAwaited: string = awaited;
                const resolved = SnapshotPromise.resolve(1);
                const numberControl: SnapshotPromise<number> = resolved;
                resolved.then(value => {
                    const valueControl: number = value;
                    const wrongValue: string = value;
                    return value;
                });
                const wrongPromise: SnapshotPromise<string> = resolved;
            "#,
        )
        .expect("focused Promise suffix checks");
        assert!(
            run.result.incomplete.is_empty(),
            "{:?}",
            run.result.incomplete
        );
        assert_eq!(
            run.result.diagnostics.len(),
            3,
            "{:?}",
            run.result.diagnostics
        );
        assert_eq!(
            run.result
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            [
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
            ],
        );
    }

    #[test]
    fn owned_conditional_awaited_preserves_non_thenable_argument() {
        let library = r#"
            type SnapshotAwaited<T> = T extends null | undefined ? T :
                T extends object & { then(onfulfilled: infer F, ...args: infer _): any; } ?
                    F extends ((value: infer V, ...args: infer _) => any) ?
                        SnapshotAwaited<V> :
                    never :
                T;
            interface SnapshotPromise<T> {
                then<Result = T>(
                    onfulfilled?: ((value: T) => Result) | null,
                ): SnapshotPromise<Result>;
            }
            interface SnapshotPromiseConstructor {
                resolve<T>(value: T): SnapshotPromise<SnapshotAwaited<T>>;
            }
            declare var SnapshotPromise: SnapshotPromiseConstructor;
        "#;
        let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "promise-awaited.d.ts",
            source: library,
        }])
        .expect("focused conditional Awaited library compiles");
        let run = check_caller_certified_collision_free_source_with_owned_library(
            state,
            r#"
                declare const awaited: SnapshotAwaited<number>;
                const wrongAwaited: string = awaited;
                const resolved = SnapshotPromise.resolve(1);
                const numberControl: SnapshotPromise<number> = resolved;
                resolved.then(value => {
                    const valueControl: number = value;
                    const wrongValue: string = value;
                    return value;
                });
                const wrongPromise: SnapshotPromise<string> = resolved;
            "#,
        )
        .expect("focused conditional Awaited suffix checks");
        assert!(
            run.result.incomplete.is_empty(),
            "{:?}",
            run.result.incomplete
        );
        assert_eq!(
            run.result.diagnostics.len(),
            3,
            "{:?}",
            run.result.diagnostics
        );
        assert_eq!(
            run.result
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            [
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
            ],
        );
    }

    fn owned_state_identity_ends(state: &OwnedLibraryRuntimeState) -> OwnedBaseFinalIdentityEnds {
        OwnedBaseFinalIdentityEnds {
            store: state.interner.store().len(),
            declared_recipes: state.interner.store().snapshot_declared_recipes().count(),
            type_params: usize::try_from(state.next_type_param)
                .expect("base type parameter end fits usize"),
            classes: usize::try_from(state.next_class_id).expect("base class end fits usize"),
            scopes: state.binder.graph.snapshot_len(),
            symbols: state.binder.symbols.len(),
            declarations: state.binder.declarations.len(),
            type_groups: state.binder.type_groups.len(),
            namespaces: state.binder.namespaces.len(),
            value_storages: state.decl_types.len(),
        }
    }

    #[test]
    fn owned_runtime_forks_share_checker_prefixes_and_keep_empty_private_suffixes() {
        let (_, mut base) = compile_owned_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "owned-sharing.d.ts",
            source: OWNED_MINI_LIBRARY,
        }])
        .expect("owned sharing library compiles");
        base.freeze_as_library_base().expect("owned base seals");
        let first = base
            .fork_user_delta_for_test()
            .expect("first owned checker suffix");
        let second = base
            .fork_user_delta_for_test()
            .expect("second owned checker suffix");

        assert!(first.shares_checker_base_with(&second));
        assert_eq!(first.decl_types.local_len(), 0);
        assert_eq!(second.decl_types.local_len(), 0);
        assert_eq!(first.decl_types.len(), base.decl_types.len());
        assert_eq!(
            second.published_types.groups().len(),
            base.published_types.groups().len()
        );
    }

    #[test]
    fn owned_base_final_identity_witness_uses_real_aliases_dense_suffixes_and_base_shapes() {
        let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "owned-final-identities.d.ts",
            source: OWNED_MINI_LIBRARY,
        }])
        .expect("owned final-identity library compiles");
        let base = owned_state_identity_ends(&state);
        let run = check_caller_certified_collision_free_source_with_owned_library(
            state,
            r#"
                export type FirstLibraryAlias = LibraryBase<string>;
                export type SecondLibraryAlias = LibraryBase<string>;
                export declare namespace HonestLocalSpace {
                    export interface HonestLocalInterface<T> { value: T; }
                    export class HonestLocalClass<U> {
                        value: U;
                    }
                    export const localValue: HonestLocalClass<number>;
                }
                export const honestReusedFunction = increment;
            "#,
        )
        .expect("owned base captures final identity state");
        assert!(
            run.result.diagnostics.is_empty(),
            "{:?}",
            run.result.diagnostics
        );
        assert!(
            run.result.incomplete.is_empty(),
            "{:?}",
            run.result.incomplete
        );

        let aliases = &run.final_identity.named_alias_types;
        assert_eq!(
            aliases.get("FirstLibraryAlias"),
            aliases.get("SecondLibraryAlias")
        );
        assert!(aliases.get("FirstLibraryAlias").is_some());
        for (base_end, final_end) in [
            (base.store, run.final_identity.ends.store),
            (base.type_params, run.final_identity.ends.type_params),
            (base.classes, run.final_identity.ends.classes),
            (base.scopes, run.final_identity.ends.scopes),
            (base.symbols, run.final_identity.ends.symbols),
            (base.declarations, run.final_identity.ends.declarations),
            (base.type_groups, run.final_identity.ends.type_groups),
            (base.namespaces, run.final_identity.ends.namespaces),
            (base.value_storages, run.final_identity.ends.value_storages),
        ] {
            assert!(final_end > base_end, "{base_end}..{final_end}");
        }
        let reused = run
            .final_identity
            .reused_base_shape
            .expect("user lowering reuses a non-intrinsic base shape");
        assert!(reused.type_id.index() < base.store);
        assert!(matches!(
            reused.tag,
            TypeTag::Object | TypeTag::ClassInstance | TypeTag::Function
        ));
    }

    #[test]
    fn production_check_program_does_not_enter_final_identity_inspection_route() {
        let allocator = Allocator::default();
        let parsed = Parser::new(
            &allocator,
            "type ProductionAlias = { value: number }; const value: ProductionAlias = { value: 1 };",
            SourceType::ts(),
        )
        .parse();
        assert!(!parsed.panicked);
        assert!(parsed.diagnostics.is_empty());
        let inspections_before = super::super::final_identity_inspector_calls_for_test();
        let mut interner = Interner::with_intrinsics();
        let result = super::super::check_program(&mut interner, &parsed.program);
        assert!(result.diagnostics.is_empty());
        assert!(result.incomplete.is_empty());
        assert_eq!(
            super::super::final_identity_inspector_calls_for_test(),
            inspections_before
        );
    }

    #[test]
    fn owned_library_state_checks_caller_certified_collision_free_suffix_without_prefix_rebinding()
    {
        let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "owned-mini.d.ts",
            source: OWNED_MINI_LIBRARY,
        }])
        .expect("owned mini library compiles");
        let run =
            check_caller_certified_collision_free_source_with_owned_library_and_verify_prefix(
                state,
                OWNED_MINI_USER,
            )
            .expect("owned mini base accepts a caller-certified collision-free suffix");
        let codes = run
            .result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>();
        assert_eq!(
            codes,
            [
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2339,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2339,
            ]
        );
        assert_eq!(
            run.result
                .diagnostics
                .last()
                .map(|diagnostic| diagnostic.message.as_str()),
            Some("Property 'notAFunctionMember' does not exist on type 'typeof increment'")
        );
        assert!(run.result.incomplete.is_empty());
        assert!(run.witness.array_group_stable);
        assert!(run.witness.document_value_stable);
        assert_eq!(run.witness.store_prefix_stable, Some(true));
        assert!(run.witness.final_store_len >= run.witness.base_store_len);
        assert!(run.witness.final_type_group_count > run.witness.base_type_group_count);
        assert!(run.witness.final_decl_count > run.witness.base_decl_count);
        assert!(run.witness.source_key > run.witness.base_max_source_key);
    }

    #[test]
    fn owned_frozen_interface_heritage_projects_into_local_interface() {
        let (_, state) = compile_owned_injected_profile(&[
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(0),
                name: "owned-mini.d.ts",
                source: OWNED_MINI_LIBRARY,
            },
            InjectedLibrarySource {
                file_ordinal: LibraryFileOrdinal::new(1),
                name: "html-element.d.ts",
                source: "interface HTMLElement { elementMarker: string; }",
            },
        ])
        .expect("owned HTMLElement library compiles");
        let run = check_caller_certified_collision_free_source_with_owned_library(
            state,
            r#"interface Local extends HTMLElement { localMarker: number; }
declare const local: Local;
const inheritedGood: string = local.elementMarker;
const inheritedBad: number = local.elementMarker;
"#,
        )
        .expect("local interface composes its frozen heritage endpoint");

        assert_eq!(run.result.diagnostics.len(), 1);
        assert_eq!(
            run.result.diagnostics[0].code,
            crate::diagnostics::DiagnosticCode::TK2322,
        );
        assert!(
            run.result.incomplete.is_empty(),
            "{:#?}",
            run.result.incomplete
        );
    }

    #[test]
    fn owned_library_preserves_class_and_namespace_runtime_metadata_for_caller_certified_collision_free_suffix(
    ) {
        let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "owned-mini.d.ts",
            source: OWNED_MINI_LIBRARY,
        }])
        .expect("owned mini library compiles");
        let run =
            check_caller_certified_collision_free_source_with_owned_library_and_verify_prefix(
                state,
                r#"
                class UserFirst extends LibraryBase<string> { constructor() { super("x"); } }
                const explicit = new LibraryBase<string>("x");
                const value: string = explicit.value;
                const kind: string = LibraryBase.kind;
                const answer: number = RuntimeBag.answer;
                const RuntimeAlias = RuntimeBag;
                const aliasAnswer: number = RuntimeAlias.answer;
                new PrivateLibraryClass();
            "#,
            )
            .expect("owned class and namespace metadata installs");
        assert_eq!(run.result.diagnostics.len(), 1);
        assert_eq!(
            run.result.diagnostics[0].code,
            crate::diagnostics::DiagnosticCode::TK2673
        );
        assert!(run.result.incomplete.is_empty());
        assert_eq!(run.witness.store_prefix_stable, Some(true));
    }

    #[test]
    fn owned_library_continuation_rejects_declare_global_syntax() {
        let (_, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
            file_ordinal: LibraryFileOrdinal::new(0),
            name: "owned-mini.d.ts",
            source: OWNED_MINI_LIBRARY,
        }])
        .expect("owned mini library compiles");
        let checks_before = super::super::bound_user_check_calls_for_test();
        let error = match check_caller_certified_collision_free_source_with_owned_library(
            state,
            "export {}; declare global { interface UserOwnedGlobal { value: string; } }",
        ) {
            Ok(_) => panic!("WU5 owns declare-global continuation"),
            Err(error) => error,
        };
        assert_eq!(
            error,
            "frozen-library continuation does not yet admit declare global"
        );
        assert_eq!(
            super::super::bound_user_check_calls_for_test(),
            checks_before
        );
    }

    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "full-profile owned-base semantic selection is release-only"
    )]
    fn exact_full_profile_owned_base_checks_caller_certified_collision_free_suffix_fast_clean_and_create_element(
    ) {
        let profile = ExactLibraryProfile::load_packaged().expect("exact pinned full profile");
        let (compiled, state) =
            compile_owned_injected_profile(&injected_packaged_sources(&profile))
                .expect("exact source-compiled owned library");
        eprintln!(
            "owned-base compile timings: parse={:?} bind={:?} reserve_fill={:?} publication={:?} statements={:?} total={:?}",
            compiled.phase_timings.parse,
            compiled.phase_timings.bind,
            compiled.phase_timings.reserve_fill,
            compiled.phase_timings.publication_validation,
            compiled.phase_timings.statement_check,
            compiled.phase_timings.measured_total(),
        );
        let source = concat!(
            include_str!("../../../tooling/full-lib-bench/workloads/fast-clean/main.ts"),
            "\nconst directDomProbe: HTMLDivElement = document.createElement(\"div\");\n",
        );
        let run = check_caller_certified_collision_free_source_with_owned_library(state, source)
            .expect("exact owned base accepts the WU0A suffix");
        eprintln!(
            "owned-base user timings: parse={:?} bind={:?} check={:?}",
            run.timings.parse, run.timings.bind, run.timings.check
        );

        let (_, state) = compile_owned_injected_profile(&injected_packaged_sources(&profile))
            .expect("second exact source-compiled owned library");
        let focused = check_caller_certified_collision_free_source_with_owned_library(
            state,
            r#"
                const mutableBad: string[] = [1].map(value => value);
                const readonlyValues: readonly number[] = [1, 2];
                const readonlyBad: string[] = readonlyValues.map(value => value);
                readonlyValues.push(3);
                const upperBad: number = "x".toUpperCase();
                const fixedBad: number = (1).toFixed();
                const booleanBad: string = true.valueOf();
                const calledBad: number = ((value: string) => value).call(undefined, "x");
                const objectBad: number = ({ value: 1 }).toString();
                const regexpBad: string = /x/.test("x");
                const domBad: number = document.createElement("div");
            "#,
        )
        .expect("exact owned base accepts the focused semantic suffix");
        eprintln!(
            "owned-base focused timings: parse={:?} bind={:?} check={:?}",
            focused.timings.parse, focused.timings.bind, focused.timings.check
        );
        assert_eq!(
            focused
                .result
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            [
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2339,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
                crate::diagnostics::DiagnosticCode::TK2322,
            ]
        );
        assert!(focused.result.incomplete.is_empty());
        assert_eq!(focused.witness.store_prefix_stable, None);
        assert_eq!(run.witness.store_prefix_stable, None);
        assert!(
            run.result.incomplete.is_empty(),
            "{:?}",
            run.result.incomplete
        );
        assert!(
            run.result.diagnostics.is_empty(),
            "{:?}",
            run.result.diagnostics
        );
    }
}

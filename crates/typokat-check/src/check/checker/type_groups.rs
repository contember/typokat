use super::context::{
    ConstructionDrafts, Pass, PublishedTypeDecl, TypeDecl, TypeDeclTable, TypeResolvedTable,
};
use crate::binder::declaration::TypeGroupId;
use crate::binder::declaration::ValueStorageId;
use crate::check::query::PublishedClassLookup;
use crate::class_semantics::OwnedPublishedClassTerminal;
use crate::class_semantics::{DemandOutcome, Exhaustion, PublishedClassSurface, PublishedClasses};
use crate::types::layered::LayeredVec;
use crate::types::repr::{AliasId, ClassId, PropertyType, TypeParamId};
use crate::types::store::TypeId;
use crate::types::substitute;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use typokat_types::alias_semantics::{PublishedAliasTerminal, PublishedAliases};
pub(in crate::check::checker) struct ConstructionClassLookup<'a> {
    staged: &'a PublishedClasses,
    inherited: Option<&'a PublishedClasses>,
    staged_aliases: &'a PublishedAliases,
    inherited_aliases: Option<&'a PublishedAliases>,
    semantic_publication_identity: &'a Arc<()>,
}

impl<'a> ConstructionClassLookup<'a> {
    pub(in crate::check::checker) fn new(
        staged: &'a PublishedClasses,
        inherited: Option<&'a PublishedClasses>,
        staged_aliases: &'a PublishedAliases,
        inherited_aliases: Option<&'a PublishedAliases>,
        semantic_publication_identity: &'a Arc<()>,
    ) -> Self {
        Self {
            staged,
            inherited,
            staged_aliases,
            inherited_aliases,
            semantic_publication_identity,
        }
    }
}

impl PublishedClassLookup for ConstructionClassLookup<'_> {
    fn published_class(&self, class: ClassId) -> DemandOutcome<&PublishedClassSurface> {
        match self.staged.published_class(class) {
            outcome @ DemandOutcome::Exhausted(Exhaustion::ClassNotPublished {
                class: missing,
                ..
            }) if missing == class => self
                .inherited
                .map_or(outcome, |classes| classes.published_class(class)),
            outcome => outcome,
        }
    }

    fn require_class(&self, class: ClassId) -> DemandOutcome<()> {
        match self.staged.require(class) {
            outcome @ DemandOutcome::Exhausted(Exhaustion::ClassNotPublished {
                class: missing,
                ..
            }) if missing == class => self
                .inherited
                .map_or(outcome, |classes| classes.require(class)),
            outcome => outcome,
        }
    }

    fn publication_identity(&self) -> &Arc<()> {
        self.semantic_publication_identity
    }

    fn published_alias(&self, alias: AliasId) -> Option<&PublishedAliasTerminal> {
        self.staged_aliases.terminal(alias).or_else(|| {
            self.inherited_aliases
                .and_then(|aliases| aliases.terminal(alias))
        })
    }

    fn published_class_lookup_is_observational(&self) -> bool {
        false
    }
}

fn project_local_row_is_writable<T>(rows: &LayeredVec<T>, index: usize) -> bool {
    index >= rows.base_len() && index < rows.len()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum PublishedTypeGroupSurface {
    Template(TypeId),
    Class(ClassId),
    Enum(TypeId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct PublishedTypeGroup {
    pub(in crate::check::checker) name: String,
    pub(in crate::check::checker) surface: PublishedTypeGroupSurface,
    pub(in crate::check::checker) parameters: Vec<TypeParamId>,
    pub(in crate::check::checker) parameter_names: Vec<String>,
    pub(in crate::check::checker) parameter_defaults: Vec<PublishedTypeParameterDefault>,
    pub(in crate::check::checker) conflict_alternatives: Vec<InterfaceTypedAlternative>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum InterfaceAlternativeKind {
    Member,
    StringIndex,
    NumberIndex,
    Heritage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct InterfaceTypedAlternative {
    pub(in crate::check::checker) kind: InterfaceAlternativeKind,
    pub(in crate::check::checker) key: String,
    pub(in crate::check::checker) types: Vec<TypeId>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum PublishedTypeParameterDefault {
    Absent,
    Ready(TypeId),
    Unsupported,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum TypeGroupUnavailableCause {
    UnsupportedComposition,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct PublishedTypeGroupUnavailable {
    pub(in crate::check::checker) cause: TypeGroupUnavailableCause,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum PublishedTypeGroupTerminal {
    Ready(PublishedTypeGroup),
    Unavailable(PublishedTypeGroupUnavailable),
}

#[derive(Default)]
pub(in crate::check::checker) struct PublishedTypeGroups {
    entries: LayeredVec<PublishedTypeGroupTerminal>,
    declarations: LayeredVec<PublishedTypeDecl>,
    resolved: LayeredVec<Option<TypeId>>,
}

impl PublishedTypeGroups {
    pub(in crate::check::checker) fn empty() -> Self {
        Self::default()
    }

    pub(in crate::check::checker) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(in crate::check::checker) fn get(
        &self,
        group: TypeGroupId,
    ) -> Option<&PublishedTypeGroupTerminal> {
        self.entries.get(group.index())
    }
}

/// The only query-visible type environment. Class and named-type registries become
/// visible together through one assignment after both private builders are frozen.
pub(in crate::check::checker) struct PublishedTypeEnvironment {
    classes: PublishedClasses,
    aliases: PublishedAliases,
    groups: PublishedTypeGroups,
    semantic_publication_identity: Arc<()>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct PublishedTypeEnvironmentProductParts {
    pub(in crate::check::checker) classes: Vec<(ClassId, OwnedPublishedClassTerminal)>,
    pub(in crate::check::checker) aliases: Vec<(AliasId, PublishedAliasTerminal)>,
    pub(in crate::check::checker) groups: Vec<PublishedTypeGroupTerminal>,
}

pub(in crate::check::checker) enum TypeEnvironmentState<'ast> {
    Constructing {
        inherited: Option<PublishedTypeEnvironment>,
        drafts: Option<Box<ConstructionDrafts<'ast>>>,
    },
    Published(PublishedTypeEnvironment),
}

impl<'ast> TypeEnvironmentState<'ast> {
    pub(in crate::check::checker) fn constructing(drafts: ConstructionDrafts<'ast>) -> Self {
        Self::Constructing {
            inherited: Some(PublishedTypeEnvironment::empty()),
            drafts: Some(Box::new(drafts)),
        }
    }

    pub(in crate::check::checker) fn inherited(&self) -> &PublishedTypeEnvironment {
        match self {
            Self::Constructing {
                inherited: Some(inherited),
                ..
            } => inherited,
            Self::Constructing {
                inherited: None, ..
            } => panic!("inherited environment is being consumed"),
            Self::Published(_) => panic!("published phase has no construction-only inherited view"),
        }
    }

    fn inherited_mut(&mut self) -> Option<&mut PublishedTypeEnvironment> {
        match self {
            Self::Constructing {
                inherited: Some(inherited),
                ..
            } => Some(inherited),
            Self::Constructing {
                inherited: None, ..
            }
            | Self::Published(_) => None,
        }
    }

    pub(in crate::check::checker) fn published(&self) -> &PublishedTypeEnvironment {
        match self {
            Self::Published(environment) => environment,
            Self::Constructing { .. } => {
                panic!("production type environment is unavailable during construction")
            }
        }
    }

    pub(in crate::check::checker) fn resolution_environment(&self) -> &PublishedTypeEnvironment {
        match self {
            Self::Constructing { .. } => self.inherited(),
            Self::Published(_) => self.published(),
        }
    }

    pub(in crate::check::checker) fn semantic_query_class_layers(
        &self,
    ) -> (
        &PublishedClasses,
        Option<&PublishedClasses>,
        &PublishedAliases,
        Option<&PublishedAliases>,
        &Arc<()>,
    ) {
        match self {
            Self::Constructing {
                inherited: Some(inherited),
                drafts: Some(drafts),
            } => (
                drafts
                    .staged_published_classes
                    .as_ref()
                    .expect("semantic construction query requires staged class terminals"),
                Some(inherited.classes()),
                drafts
                    .staged_published_aliases
                    .as_ref()
                    .expect("semantic construction query requires staged alias terminals"),
                Some(inherited.aliases()),
                inherited.semantic_publication_identity(),
            ),
            Self::Constructing {
                inherited: None, ..
            } => panic!("inherited environment is being consumed"),
            Self::Constructing { drafts: None, .. } => {
                panic!("construction drafts are being consumed")
            }
            Self::Published(environment) => (
                environment.classes(),
                None,
                environment.aliases(),
                None,
                environment.semantic_publication_identity(),
            ),
        }
    }

    pub(in crate::check::checker) fn is_published(&self) -> bool {
        matches!(self, Self::Published(_))
    }

    pub(in crate::check::checker) fn drafts(&self) -> &ConstructionDrafts<'ast> {
        match self {
            Self::Constructing {
                drafts: Some(drafts),
                ..
            } => drafts,
            Self::Constructing { drafts: None, .. } => {
                panic!("construction drafts are being consumed")
            }
            Self::Published(_) => {
                panic!("construction drafts are unavailable after atomic publication")
            }
        }
    }

    pub(in crate::check::checker) fn drafts_mut(&mut self) -> &mut ConstructionDrafts<'ast> {
        match self {
            Self::Constructing {
                drafts: Some(drafts),
                ..
            } => drafts,
            Self::Constructing { drafts: None, .. } => {
                panic!("construction drafts are being consumed")
            }
            Self::Published(_) => {
                panic!("construction drafts are unavailable after atomic publication")
            }
        }
    }

    fn publish(
        &mut self,
        groups: PublishedTypeGroups,
        staged_classes: PublishedClasses,
        staged_aliases: PublishedAliases,
    ) {
        let (inherited, drafts) = match self {
            Self::Constructing { inherited, drafts } => (
                inherited
                    .take()
                    .expect("inherited environment is consumed exactly once"),
                drafts
                    .take()
                    .expect("construction drafts are consumed exactly once"),
            ),
            Self::Published(_) => panic!("type environment publishes exactly once"),
        };
        assert!(drafts.type_group_construction.is_none());
        assert!(drafts.staged_published_classes.is_none());
        assert!(drafts.staged_published_aliases.is_none());
        let classes = inherited
            .classes
            .extend(staged_classes)
            .expect("class publication epochs must be disjoint");
        let aliases = inherited
            .aliases
            .extend(staged_aliases)
            .expect("alias publication epochs must be disjoint");
        *self = Self::Published(PublishedTypeEnvironment {
            classes,
            aliases,
            groups,
            semantic_publication_identity: Arc::new(()),
        });
    }
}

impl<Ticket: Copy + PartialEq> Pass<'_, '_, Ticket> {
    pub(in crate::check::checker) fn project_local_native_array_alias(
        &self,
        group: TypeGroupId,
    ) -> Option<super::library_identities::NativeArrayAlias> {
        if !self.project_local_native_array_aliases_enabled {
            return None;
        }
        let groups = self.project_local_library_identity_groups?;
        if groups[0] == Some(group) {
            return Some(super::library_identities::NativeArrayAlias::Array);
        }
        if groups[1] == Some(group) {
            return Some(super::library_identities::NativeArrayAlias::ReadonlyArray);
        }
        None
    }
    pub(in crate::check::checker) fn project_local_type_group_is_ready(
        &self,
        group: TypeGroupId,
    ) -> bool {
        matches!(
            self.type_environment
                .resolution_environment()
                .groups
                .get(group),
            Some(PublishedTypeGroupTerminal::Ready(_))
        )
    }

    pub(in crate::check::checker) fn demand_project_local_type_group(
        &mut self,
        group: TypeGroupId,
    ) -> bool {
        let Some(gateway) = self.project_local_demand_gateway.take() else {
            return true;
        };
        let snapshot = gateway.demand_type_group(self.interner, group);
        self.project_local_demand_gateway = Some(gateway);
        let result =
            snapshot.and_then(|snapshot| self.install_project_local_demand_snapshot(snapshot));
        if let Err(error) = result {
            self.project_local_demand_error.get_or_insert(error);
            return false;
        }
        true
    }

    pub(in crate::check::checker) fn demand_project_local_value(
        &mut self,
        storage: ValueStorageId,
    ) -> bool {
        let Some(gateway) = self.project_local_demand_gateway.take() else {
            return true;
        };
        let snapshot = gateway.demand_value(self.interner, storage);
        self.project_local_demand_gateway = Some(gateway);
        let result =
            snapshot.and_then(|snapshot| self.install_project_local_demand_snapshot(snapshot));
        if let Err(error) = result {
            self.project_local_demand_error.get_or_insert(error);
            return false;
        }
        true
    }

    pub(in crate::check::checker) fn demand_project_local_global_values(&mut self) -> bool {
        let Some(gateway) = self.project_local_demand_gateway.take() else {
            return true;
        };
        let snapshot = gateway.demand_global_values(self.interner);
        self.project_local_demand_gateway = Some(gateway);
        if let Err(error) =
            snapshot.and_then(|snapshot| self.install_project_local_demand_snapshot(snapshot))
        {
            self.project_local_demand_error.get_or_insert(error);
            return false;
        }
        let mut object = self
            .global_object_type
            .and_then(|ty| self.interner.store().object_type(ty).cloned())
            .unwrap_or_default();
        for (name, storage) in &self.project_local_library_global_values {
            let Some(ty) = self.decl_types.get(*storage) else {
                continue;
            };
            let property = PropertyType::public(name.clone(), ty);
            match object
                .properties
                .iter_mut()
                .find(|existing| existing.key == property.key)
            {
                Some(existing) => *existing = property,
                None => object.properties.push(property),
            }
        }
        self.global_object_type = Some(self.interner.intern_object(object));
        true
    }

    pub(in crate::check::checker) fn demand_project_local_library_identity(
        &mut self,
        index: usize,
    ) -> bool {
        let Some(group) = self
            .project_local_library_identity_groups
            .and_then(|groups| groups.get(index).copied().flatten())
        else {
            self.project_local_demand_error
                .get_or_insert("project-local library identity has no mapped product terminal");
            return false;
        };
        self.demand_project_local_type_group(group)
    }

    pub(in crate::check::checker) fn commit_project_local_staged_terminals(&mut self) -> bool {
        let Some(gateway) = self.project_local_demand_gateway.take() else {
            return true;
        };
        let snapshot = gateway.commit_staged();
        self.project_local_demand_gateway = Some(gateway);
        let result =
            snapshot.and_then(|snapshot| self.install_project_local_demand_snapshot(snapshot));
        if let Err(error) = result {
            self.project_local_demand_error.get_or_insert(error);
        }
        self.project_local_demand_error.is_none()
    }

    fn install_project_local_demand_snapshot(
        &mut self,
        snapshot: super::context::ProjectLocalDemandSnapshot,
    ) -> Result<(), &'static str> {
        let super::context::ProjectLocalDemandSnapshot {
            aliases,
            type_terminals,
            value_types,
            namespace_seeds,
        } = snapshot;
        match &self.type_environment {
            TypeEnvironmentState::Constructing {
                inherited: Some(inherited),
                drafts: Some(drafts),
            } => {
                inherited.validate_project_local_snapshot(&aliases, &type_terminals)?;
                if type_terminals.iter().any(|(group, _)| {
                    !drafts
                        .type_decls
                        .can_install_project_local_published(group.index())
                        || !drafts
                            .type_resolved
                            .can_install_project_local_published(group.index())
                }) {
                    return Err("project-local draft terminal is outside the published prefix");
                }
            }
            TypeEnvironmentState::Published(environment) => {
                environment.validate_project_local_snapshot(&aliases, &type_terminals)?;
            }
            TypeEnvironmentState::Constructing { .. } => {
                return Err("project-local type environment is being published")
            }
        }
        match &mut self.type_environment {
            TypeEnvironmentState::Constructing {
                inherited: Some(inherited),
                drafts: Some(drafts),
            } => {
                inherited.install_project_local_alias_snapshot(&aliases)?;
                for (group, terminal) in type_terminals {
                    let (declaration, resolution) = construction_terminal(&terminal);
                    drafts
                        .type_group_construction
                        .as_mut()
                        .ok_or("project-local type-group construction is unavailable")?
                        .install_project_local_terminal(group, terminal.clone())?;
                    inherited.install_project_local_terminal(group, terminal)?;
                    drafts
                        .type_decls
                        .install_project_local_published(group.index(), declaration)?;
                    drafts
                        .type_resolved
                        .install_project_local_published(group.index(), resolution)?;
                }
            }
            TypeEnvironmentState::Published(environment) => {
                environment.install_project_local_alias_snapshot(&aliases)?;
                for (group, terminal) in type_terminals {
                    environment.install_project_local_terminal(group, terminal)?;
                }
            }
            TypeEnvironmentState::Constructing { .. } => {
                return Err("project-local type environment is being published")
            }
        }
        for (storage, ty) in value_types {
            self.decl_types.set(storage, ty);
        }
        let user_modules = self
            .project_local_user_modules
            .clone()
            .ok_or("project-local user modules are unavailable")?;
        self.install_project_local_namespace_seeds(namespace_seeds, &user_modules)?;
        if let Some(groups) = self.project_local_library_identity_groups {
            let identities =
                super::library_identities::LibrarySemanticIdentities::select_project_local_exact(
                    groups,
                    self.type_environment.resolution_environment(),
                    self.interner.store(),
                );
            self.semantic_queries
                .set_library_object_context(identities.object_relation_context());
            self.library_semantic_identities = Some(identities);
        }
        Ok(())
    }
}

impl PublishedTypeEnvironment {
    pub(in crate::check::checker) fn empty() -> Self {
        Self {
            classes: PublishedClasses::empty(),
            aliases: PublishedAliases::empty(),
            groups: PublishedTypeGroups::empty(),
            semantic_publication_identity: Arc::new(()),
        }
    }

    #[cfg(test)]
    pub(in crate::check::checker) fn from_project_local_terminals(
        entries: Vec<PublishedTypeGroupTerminal>,
    ) -> Self {
        Self::from_project_local_terminals_with_aliases(entries, PublishedAliases::empty())
    }

    pub(in crate::check::checker) fn from_project_local_terminals_with_aliases(
        entries: Vec<PublishedTypeGroupTerminal>,
        aliases: PublishedAliases,
    ) -> Self {
        Self {
            classes: PublishedClasses::empty(),
            aliases,
            groups: published_type_groups_from_terminals(entries),
            semantic_publication_identity: Arc::new(()),
        }
    }

    #[cfg(test)]
    pub(in crate::check::checker) fn from_explicit_terminals_for_test(
        entries: Vec<PublishedTypeGroupTerminal>,
    ) -> Self {
        Self {
            classes: PublishedClasses::empty(),
            aliases: PublishedAliases::empty(),
            groups: published_type_groups_from_terminals(entries),
            semantic_publication_identity: Arc::new(()),
        }
    }

    pub(in crate::check::checker) fn classes(&self) -> &PublishedClasses {
        &self.classes
    }

    pub(in crate::check::checker) fn aliases(&self) -> &PublishedAliases {
        &self.aliases
    }

    pub(in crate::check::checker) fn semantic_publication_identity(&self) -> &Arc<()> {
        &self.semantic_publication_identity
    }

    pub(in crate::check::checker) fn groups(&self) -> &PublishedTypeGroups {
        &self.groups
    }

    fn validate_project_local_snapshot(
        &self,
        aliases: &PublishedAliases,
        groups: &[(TypeGroupId, PublishedTypeGroupTerminal)],
    ) -> Result<(), &'static str> {
        self.aliases.validate_snapshot(aliases)?;
        let valid_group = |group: TypeGroupId| {
            let index = group.index();
            project_local_row_is_writable(&self.groups.entries, index)
                && project_local_row_is_writable(&self.groups.declarations, index)
                && project_local_row_is_writable(&self.groups.resolved, index)
        };
        if groups.iter().any(|(group, _)| !valid_group(*group)) {
            return Err("project-local terminal is outside the published prefix");
        }
        Ok(())
    }

    fn install_project_local_alias_snapshot(
        &mut self,
        aliases: &PublishedAliases,
    ) -> Result<(), &'static str> {
        if self.aliases.install_snapshot(aliases)? {
            self.semantic_publication_identity = Arc::new(());
        }
        Ok(())
    }

    pub(in crate::check::checker) fn install_project_local_terminal(
        &mut self,
        group: TypeGroupId,
        terminal: PublishedTypeGroupTerminal,
    ) -> Result<(), &'static str> {
        let (declaration, resolution) = construction_terminal(&terminal);
        *self
            .groups
            .entries
            .get_mut_local(group.index())
            .ok_or("project-local terminal is outside the published prefix")? = terminal;
        *self
            .groups
            .declarations
            .get_mut_local(group.index())
            .ok_or("project-local declaration is outside the published prefix")? = declaration;
        *self
            .groups
            .resolved
            .get_mut_local(group.index())
            .ok_or("project-local resolution is outside the published prefix")? = resolution;
        Ok(())
    }
    fn replace_project_local_class_surface(
        &mut self,
        replacement: PublishedClassSurface,
    ) -> Result<(), &'static str> {
        let class = replacement.class();
        let mut terminals = self
            .classes
            .owned_terminals()
            .ok_or("published class registry contains a non-terminal state")?;
        let terminal = terminals
            .iter_mut()
            .find(|(candidate, _)| *candidate == class)
            .ok_or("project-local class surface is outside the published registry")?;
        if !matches!(terminal.1, OwnedPublishedClassTerminal::Ready(_)) {
            return Err("project-local class surface cannot replace a poisoned class");
        }
        terminal.1 = OwnedPublishedClassTerminal::Ready(replacement);
        self.classes = PublishedClasses::from_owned_terminals(terminals)?;
        self.semantic_publication_identity = Arc::new(());
        Ok(())
    }

    pub(in crate::check::checker) fn construction_prefix<'ast>(
        &self,
    ) -> (TypeDeclTable<'ast>, TypeResolvedTable) {
        (
            TypeDeclTable::with_published(self.groups.declarations.clone()),
            TypeResolvedTable::with_published(self.groups.resolved.clone()),
        )
    }

    pub(in crate::check::checker) fn product_parts(
        &self,
    ) -> Result<PublishedTypeEnvironmentProductParts, &'static str> {
        Ok(PublishedTypeEnvironmentProductParts {
            classes: self
                .classes
                .owned_terminals()
                .ok_or("class publication contains a non-terminal state")?,
            aliases: self.aliases.owned_terminals(),
            groups: self.groups.entries.iter().cloned().collect(),
        })
    }
}

impl PublishedClassLookup for PublishedTypeEnvironment {
    fn published_class(&self, class: ClassId) -> DemandOutcome<&PublishedClassSurface> {
        self.classes.published_class(class)
    }

    fn require_class(&self, class: ClassId) -> DemandOutcome<()> {
        self.classes.require(class)
    }

    fn publication_identity(&self) -> &Arc<()> {
        &self.semantic_publication_identity
    }

    fn published_alias(&self, alias: AliasId) -> Option<&PublishedAliasTerminal> {
        self.aliases.terminal(alias)
    }

    fn published_class_lookup_is_observational(&self) -> bool {
        false
    }
}

fn published_type_groups_from_terminals(
    terminals: Vec<PublishedTypeGroupTerminal>,
) -> PublishedTypeGroups {
    let mut declarations = Vec::with_capacity(terminals.len());
    let mut resolved = Vec::with_capacity(terminals.len());
    for terminal in &terminals {
        let (declaration, resolution) = construction_terminal(terminal);
        declarations.push(declaration);
        resolved.push(resolution);
    }
    PublishedTypeGroups {
        entries: terminals.into(),
        declarations: declarations.into(),
        resolved: resolved.into(),
    }
}

fn construction_terminal(
    terminal: &PublishedTypeGroupTerminal,
) -> (PublishedTypeDecl, Option<TypeId>) {
    match terminal {
        PublishedTypeGroupTerminal::Ready(group) => (
            PublishedTypeDecl {
                params: group.parameters.clone(),
                defaults: group.parameter_defaults.clone(),
            },
            match group.surface {
                PublishedTypeGroupSurface::Template(template) => Some(template),
                PublishedTypeGroupSurface::Enum(enumeration) => Some(enumeration),
                PublishedTypeGroupSurface::Class(_) => None,
            },
        ),
        PublishedTypeGroupTerminal::Unavailable(_) => (
            PublishedTypeDecl {
                params: Vec::new(),
                defaults: Vec::new(),
            },
            None,
        ),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TypeGroupConstructionSlot {
    Pending,
    Building,
    Frozen(PublishedTypeGroupTerminal),
}

enum InheritedClassAugmentation {
    NotClass,
    Ready(PublishedTypeGroup),
    Unavailable,
}

enum InheritedTemplateAugmentation {
    NotTemplate,
    Ready(PublishedTypeGroup),
    Unavailable,
}

/// Private, single-use construction authority for one exact type-group epoch.
pub(in crate::check::checker) struct TypeGroupConstruction {
    expected: usize,
    base: Option<PublishedTypeGroups>,
    base_len: usize,
    slots: Vec<TypeGroupConstructionSlot>,
    replacements: FxHashMap<usize, TypeGroupConstructionSlot>,
}

impl TypeGroupConstruction {
    pub(in crate::check::checker) fn new(group_count: usize) -> Self {
        Self {
            expected: group_count,
            base: None,
            base_len: 0,
            slots: Vec::new(),
            replacements: FxHashMap::default(),
        }
    }

    fn install_base(&mut self, base: &PublishedTypeGroups) {
        assert!(self.base.is_none());
        assert!(base.len() <= self.expected);
        self.base_len = base.len();
        self.base = Some(PublishedTypeGroups {
            entries: base.entries.iter().cloned().collect::<Vec<_>>().into(),
            declarations: base.declarations.iter().cloned().collect::<Vec<_>>().into(),
            resolved: base.resolved.iter().copied().collect::<Vec<_>>().into(),
        });
        if self.slots.is_empty() {
            self.slots = vec![TypeGroupConstructionSlot::Pending; self.expected - self.base_len];
        } else {
            assert_eq!(self.slots.len(), self.expected);
            let suffix = self.slots.split_off(self.base_len);
            assert!(self
                .slots
                .iter()
                .all(|slot| *slot == TypeGroupConstructionSlot::Pending));
            self.slots = suffix;
        }
    }

    fn install_project_local_terminal(
        &mut self,
        group: TypeGroupId,
        terminal: PublishedTypeGroupTerminal,
    ) -> Result<(), &'static str> {
        let base = self
            .base
            .as_mut()
            .ok_or("project-local type-group base is unavailable")?;
        let (declaration, resolution) = construction_terminal(&terminal);
        *base
            .entries
            .get_mut_local(group.index())
            .ok_or("project-local terminal is outside the published prefix")? = terminal;
        *base
            .declarations
            .get_mut_local(group.index())
            .ok_or("project-local declaration is outside the published prefix")? = declaration;
        *base
            .resolved
            .get_mut_local(group.index())
            .ok_or("project-local resolution is outside the published prefix")? = resolution;
        Ok(())
    }

    fn begin(&mut self, group: TypeGroupId) {
        if self.base.is_none() && self.slots.is_empty() {
            self.slots = vec![TypeGroupConstructionSlot::Pending; self.expected];
        }
        if group.index() < self.base_len {
            let slot = self
                .replacements
                .entry(group.index())
                .or_insert(TypeGroupConstructionSlot::Pending);
            assert_eq!(*slot, TypeGroupConstructionSlot::Pending);
            *slot = TypeGroupConstructionSlot::Building;
            return;
        }
        let slot = self
            .slots
            .get_mut(
                group
                    .index()
                    .checked_sub(self.base_len)
                    .expect("installed base type groups cannot be reconstructed"),
            )
            .expect("type group construction id must be dense");
        assert_eq!(
            *slot,
            TypeGroupConstructionSlot::Pending,
            "type group {group:?} must not reconstruct an installed base entry"
        );
        *slot = TypeGroupConstructionSlot::Building;
    }

    fn freeze(&mut self, group: TypeGroupId, terminal: PublishedTypeGroupTerminal) {
        if group.index() < self.base_len {
            match self.replacements.entry(group.index()) {
                std::collections::hash_map::Entry::Occupied(mut entry)
                    if *entry.get() == TypeGroupConstructionSlot::Building =>
                {
                    entry.insert(TypeGroupConstructionSlot::Frozen(terminal));
                }
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    entry.insert(TypeGroupConstructionSlot::Pending);
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(TypeGroupConstructionSlot::Pending);
                }
            }
            return;
        }
        let slot = self
            .slots
            .get_mut(
                group
                    .index()
                    .checked_sub(self.base_len)
                    .expect("installed base type groups cannot be reconstructed"),
            )
            .expect("type group construction id must be dense");
        assert_eq!(*slot, TypeGroupConstructionSlot::Building);
        *slot = TypeGroupConstructionSlot::Frozen(terminal);
    }

    fn is_pending(&self, group: TypeGroupId) -> bool {
        if group.index() < self.base_len {
            return self
                .replacements
                .get(&group.index())
                .is_some_and(|slot| *slot == TypeGroupConstructionSlot::Pending);
        }
        if self.slots.is_empty() {
            return group.index() < self.expected;
        }
        self.slots
            .get(group.index() - self.base_len)
            .is_some_and(|slot| *slot == TypeGroupConstructionSlot::Pending)
    }

    fn has_replacement(&self, group: TypeGroupId) -> bool {
        self.replacements.contains_key(&group.index())
    }

    fn is_frozen(&self, group: TypeGroupId) -> bool {
        if group.index() < self.base_len {
            return self
                .replacements
                .get(&group.index())
                .is_none_or(|slot| matches!(slot, TypeGroupConstructionSlot::Frozen(_)));
        }
        self.slots
            .get(group.index() - self.base_len)
            .is_some_and(|slot| matches!(slot, TypeGroupConstructionSlot::Frozen(_)))
    }

    fn consume(self, expected: usize) -> Option<PublishedTypeGroups> {
        if self.expected != expected || self.base_len + self.slots.len() != expected {
            return None;
        }
        let (mut entries, mut declarations, mut resolved) = match self.base {
            Some(base) => (base.entries, base.declarations, base.resolved),
            None => (
                LayeredVec::default(),
                LayeredVec::default(),
                LayeredVec::default(),
            ),
        };
        for (index, slot) in self.replacements {
            let TypeGroupConstructionSlot::Frozen(terminal) = slot else {
                return None;
            };
            let (declaration, resolution) = construction_terminal(&terminal);
            *entries.get_mut_local(index)? = terminal;
            *declarations.get_mut_local(index)? = declaration;
            *resolved.get_mut_local(index)? = resolution;
        }
        for slot in self.slots {
            let TypeGroupConstructionSlot::Frozen(terminal) = slot else {
                return None;
            };
            let (declaration, resolution) = construction_terminal(&terminal);
            entries.push_local(terminal);
            declarations.push_local(declaration);
            resolved.push_local(resolution);
        }
        Some(PublishedTypeGroups {
            entries,
            declarations,
            resolved,
        })
    }

    fn unfinished_groups(&self) -> Vec<(usize, &'static str)> {
        let mut unfinished = self
            .replacements
            .iter()
            .filter_map(|(index, slot)| match slot {
                TypeGroupConstructionSlot::Pending => Some((*index, "pending")),
                TypeGroupConstructionSlot::Building => Some((*index, "building")),
                TypeGroupConstructionSlot::Frozen(_) => None,
            })
            .collect::<Vec<_>>();
        unfinished.extend(
            self.slots
                .iter()
                .enumerate()
                .filter_map(|(index, slot)| match slot {
                    TypeGroupConstructionSlot::Pending => Some((self.base_len + index, "pending")),
                    TypeGroupConstructionSlot::Building => {
                        Some((self.base_len + index, "building"))
                    }
                    TypeGroupConstructionSlot::Frozen(_) => None,
                })
                .collect::<Vec<_>>(),
        );
        unfinished.sort_by_key(|(index, _)| *index);
        unfinished
    }
}

fn parameter_names(
    declaration: Option<&oxc_ast::ast::TSTypeParameterDeclaration<'_>>,
) -> Vec<String> {
    declaration
        .iter()
        .flat_map(|declaration| declaration.params.iter())
        .map(|parameter| parameter.name.name.to_string())
        .collect()
}

fn parameter_defaults(
    declaration: Option<&oxc_ast::ast::TSTypeParameterDeclaration<'_>>,
    lowered: &[Option<TypeId>],
) -> Vec<PublishedTypeParameterDefault> {
    declaration
        .iter()
        .flat_map(|declaration| declaration.params.iter())
        .enumerate()
        .map(
            |(index, parameter)| match (parameter.default.as_ref(), lowered.get(index)) {
                (None, _) => PublishedTypeParameterDefault::Absent,
                (Some(_), Some(Some(default))) => PublishedTypeParameterDefault::Ready(*default),
                (Some(_), _) => PublishedTypeParameterDefault::Unsupported,
            },
        )
        .collect()
}

impl<'a, 'ast, Ticket: Copy + PartialEq> Pass<'a, 'ast, Ticket> {
    fn augment_inherited_template_surface(
        &mut self,
        group: TypeGroupId,
        augmentation: TypeId,
        augmentation_parameters: &[TypeParamId],
        conflict_alternatives: &[InterfaceTypedAlternative],
    ) -> InheritedTemplateAugmentation {
        let inherited = self.type_environment.inherited().groups().get(group);
        let Some(PublishedTypeGroupTerminal::Ready(published)) = inherited else {
            return InheritedTemplateAugmentation::NotTemplate;
        };
        let PublishedTypeGroupSurface::Template(inherited_template) = published.surface else {
            return InheritedTemplateAugmentation::NotTemplate;
        };
        let mut published = published.clone();
        if augmentation_parameters.len() != published.parameters.len() {
            return InheritedTemplateAugmentation::Unavailable;
        }
        let substitutions = augmentation_parameters
            .iter()
            .copied()
            .zip(published.parameters.iter().copied())
            .enumerate()
            .map(|(index, (source, target))| {
                let name = published
                    .parameter_names
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| format!("T{index}"));
                (source, self.interner.intern_type_param(target, name))
            })
            .collect::<FxHashMap<_, _>>();
        let augmentation = substitute(self.interner, augmentation, &substitutions);
        let objects = [inherited_template, augmentation]
            .into_iter()
            .map(|ty| self.interner.store().object_type(ty).cloned())
            .collect::<Option<Vec<_>>>();
        let Some(objects) = objects else {
            return InheritedTemplateAugmentation::Unavailable;
        };
        let merged = super::decls::interface::merge_intersection_objects(self.interner, objects);
        published.surface =
            PublishedTypeGroupSurface::Template(self.interner.intern_object(merged));
        published
            .conflict_alternatives
            .extend(
                conflict_alternatives
                    .iter()
                    .cloned()
                    .map(|mut alternative| {
                        alternative.types = alternative
                            .types
                            .into_iter()
                            .map(|ty| substitute(self.interner, ty, &substitutions))
                            .collect();
                        alternative
                    }),
            );
        InheritedTemplateAugmentation::Ready(published)
    }

    fn augment_inherited_class_surface(
        &mut self,
        group: TypeGroupId,
        augmentation: TypeId,
        augmentation_parameters: &[TypeParamId],
    ) -> InheritedClassAugmentation {
        let inherited = self.type_environment.inherited().groups().get(group);
        let Some(PublishedTypeGroupTerminal::Ready(published)) = inherited else {
            return InheritedClassAugmentation::NotClass;
        };
        let PublishedTypeGroupSurface::Class(class) = published.surface else {
            return InheritedClassAugmentation::NotClass;
        };
        let published = published.clone();
        if augmentation_parameters.len() != published.parameters.len() {
            return InheritedClassAugmentation::Unavailable;
        }
        let DemandOutcome::Ready(surface) = self
            .type_environment
            .inherited()
            .classes()
            .published_class(class)
        else {
            return InheritedClassAugmentation::Unavailable;
        };
        let surface = surface.clone();
        let substitutions = augmentation_parameters
            .iter()
            .copied()
            .zip(published.parameters.iter().copied())
            .enumerate()
            .map(|(index, (source, target))| {
                let name = published
                    .parameter_names
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| format!("T{index}"));
                (source, self.interner.intern_type_param(target, name))
            })
            .collect::<FxHashMap<_, _>>();
        let augmentation = substitute(self.interner, augmentation, &substitutions);
        let objects = [surface.instance_template(), augmentation]
            .into_iter()
            .map(|ty| self.interner.store().object_type(ty).cloned())
            .collect::<Option<Vec<_>>>();
        let Some(objects) = objects else {
            return InheritedClassAugmentation::Unavailable;
        };
        let merged = super::decls::interface::merge_intersection_objects(self.interner, objects);
        let replacement = PublishedClassSurface::new(
            class,
            surface.type_params().to_vec(),
            self.interner.intern_object(merged),
            surface.static_template(),
            surface.constructor_template(),
        );
        let replaced = self
            .type_environment
            .inherited_mut()
            .is_some_and(|environment| {
                environment
                    .replace_project_local_class_surface(replacement)
                    .is_ok()
            });
        if replaced {
            InheritedClassAugmentation::Ready(published)
        } else {
            InheritedClassAugmentation::Unavailable
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(in crate::check::checker) fn install_published_type_environment_base(
        &mut self,
        base: PublishedTypeEnvironment,
    ) {
        assert_eq!(
            base.groups.len(),
            usize::try_from(self.binder.prelude_type_group_count)
                .expect("prelude type group count fits usize")
        );
        self.install_type_environment_base(base);
    }

    pub(in crate::check::checker) fn install_project_local_type_environment_base(
        &mut self,
        base: PublishedTypeEnvironment,
    ) {
        assert!(base.groups.len() <= self.binder.type_groups.len());
        self.install_type_environment_base(base);
    }

    fn install_type_environment_base(&mut self, base: PublishedTypeEnvironment) {
        self.type_group_construction
            .as_mut()
            .expect("type-group construction is consumed exactly once")
            .install_base(&base.groups);
        let TypeEnvironmentState::Constructing { inherited, .. } = &mut self.type_environment
        else {
            panic!("base environment installs only during construction")
        };
        *inherited = Some(base);
    }

    pub(in crate::check::checker) fn begin_type_group_construction(&mut self, group: TypeGroupId) {
        self.type_group_construction
            .as_mut()
            .expect("type-group construction is consumed exactly once")
            .begin(group);
    }

    pub(in crate::check::checker) fn type_group_construction_is_pending(
        &self,
        group: TypeGroupId,
    ) -> bool {
        self.type_group_construction
            .as_ref()
            .is_some_and(|construction| {
                construction.is_pending(group)
                    || (self.type_decls.has_replacement(group.index())
                        && !construction.has_replacement(group))
            })
    }

    pub(in crate::check::checker) fn type_group_construction_is_frozen(
        &self,
        group: TypeGroupId,
    ) -> bool {
        self.type_group_construction
            .as_ref()
            .is_some_and(|construction| {
                if self.type_decls.has_replacement(group.index())
                    && !construction.has_replacement(group)
                {
                    false
                } else {
                    construction.is_frozen(group)
                }
            })
    }

    pub(in crate::check::checker) fn freeze_type_group(&mut self, group: TypeGroupId) {
        let declaration = self
            .type_decls
            .get(group.index())
            .cloned()
            .expect("type group draft must exist");
        let name = self
            .binder
            .type_groups
            .get(group)
            .expect("type group metadata must exist")
            .name
            .clone();
        let trusted_marker = self
            .type_resolved
            .get(group.index())
            .copied()
            .flatten()
            .filter(|marker| {
                let well_known = self.interner.well_known();
                well_known.is_string_intrinsic_marker(*marker)
                    || *marker == well_known.this_type
                    || *marker == well_known.omit_this_parameter
            });
        if let Some(marker) = trusted_marker {
            let (parameters, parameter_names, parameter_defaults) = match &declaration {
                TypeDecl::Interface {
                    recovery_params,
                    recovery_names,
                    recovery_defaults,
                    ..
                } => (
                    recovery_params.clone(),
                    recovery_names.clone(),
                    recovery_defaults.clone(),
                ),
                TypeDecl::Alias {
                    params,
                    defaults,
                    param_decl,
                    ..
                } => (
                    params.clone(),
                    parameter_names(*param_decl),
                    parameter_defaults(*param_decl, defaults),
                ),
                TypeDecl::Resolved { params, defaults } => {
                    (params.clone(), Vec::new(), defaults.clone())
                }
                TypeDecl::Enum { .. } | TypeDecl::Class { .. } | TypeDecl::Unavailable { .. } => {
                    unreachable!("trusted markers belong to exact library type roots")
                }
            };
            let terminal = PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
                name,
                surface: PublishedTypeGroupSurface::Template(marker),
                parameters,
                parameter_names,
                parameter_defaults,
                conflict_alternatives: Vec::new(),
            });
            self.type_group_construction
                .as_mut()
                .expect("type-group construction is consumed exactly once")
                .freeze(group, terminal);
            return;
        }
        let terminal = match &declaration {
            TypeDecl::Enum { reserved, .. } => {
                PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
                    name,
                    surface: PublishedTypeGroupSurface::Enum(*reserved),
                    parameters: Vec::new(),
                    parameter_names: Vec::new(),
                    parameter_defaults: Vec::new(),
                    conflict_alternatives: Vec::new(),
                })
            }
            TypeDecl::Interface {
                reserved,
                recovery_params,
                recovery_names,
                recovery_defaults,
                conflict_alternatives,
                project_local_overlay,
                ..
            } => {
                if let Some(overlay) = project_local_overlay {
                    if overlay.unavailable || recovery_params.len() != overlay.parameters.len() {
                        self.freeze_project_local_unavailable_type_group()
                    } else {
                        let substitutions = recovery_params
                            .iter()
                            .copied()
                            .zip(overlay.parameters.iter().copied())
                            .enumerate()
                            .map(|(position, (source, target))| {
                                let name = overlay
                                    .parameter_names
                                    .get(position)
                                    .cloned()
                                    .unwrap_or_else(|| format!("T{position}"));
                                (source, self.interner.intern_type_param(target, name))
                            })
                            .collect::<FxHashMap<_, _>>();
                        let mut alternatives = conflict_alternatives.clone();
                        for alternative in &mut alternatives {
                            alternative.types = alternative
                                .types
                                .iter()
                                .copied()
                                .map(|ty| substitute(self.interner, ty, &substitutions))
                                .collect();
                        }
                        PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
                            name,
                            surface: PublishedTypeGroupSurface::Template(*reserved),
                            parameters: overlay.parameters.clone(),
                            parameter_names: overlay.parameter_names.clone(),
                            parameter_defaults: overlay.parameter_defaults.clone(),
                            conflict_alternatives: alternatives,
                        })
                    }
                } else {
                    match self.augment_inherited_class_surface(group, *reserved, recovery_params) {
                        InheritedClassAugmentation::Ready(mut inherited) => {
                            inherited
                                .conflict_alternatives
                                .extend(conflict_alternatives.clone());
                            PublishedTypeGroupTerminal::Ready(inherited)
                        }
                        InheritedClassAugmentation::NotClass => {
                            match self.augment_inherited_template_surface(
                                group,
                                *reserved,
                                recovery_params,
                                conflict_alternatives,
                            ) {
                                InheritedTemplateAugmentation::Ready(inherited) => {
                                    PublishedTypeGroupTerminal::Ready(inherited)
                                }
                                InheritedTemplateAugmentation::NotTemplate => {
                                    PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
                                        name,
                                        surface: PublishedTypeGroupSurface::Template(*reserved),
                                        parameters: recovery_params.clone(),
                                        parameter_names: recovery_names.clone(),
                                        parameter_defaults: recovery_defaults.clone(),
                                        conflict_alternatives: conflict_alternatives.clone(),
                                    })
                                }
                                InheritedTemplateAugmentation::Unavailable => {
                                    PublishedTypeGroupTerminal::Unavailable(
                                        PublishedTypeGroupUnavailable {
                                            cause:
                                                TypeGroupUnavailableCause::UnsupportedComposition,
                                        },
                                    )
                                }
                            }
                        }
                        InheritedClassAugmentation::Unavailable => {
                            PublishedTypeGroupTerminal::Unavailable(PublishedTypeGroupUnavailable {
                                cause: TypeGroupUnavailableCause::UnsupportedComposition,
                            })
                        }
                    }
                }
            }
            TypeDecl::Alias {
                alias_template,
                alias_unavailable,
                params,
                defaults,
                param_decl,
                ..
            } => {
                if alias_unavailable.is_some() {
                    PublishedTypeGroupTerminal::Unavailable(PublishedTypeGroupUnavailable {
                        cause: TypeGroupUnavailableCause::UnsupportedComposition,
                    })
                } else {
                    PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
                        name,
                        surface: PublishedTypeGroupSurface::Template(*alias_template),
                        parameters: params.clone(),
                        parameter_names: parameter_names(*param_decl),
                        parameter_defaults: parameter_defaults(*param_decl, defaults),
                        conflict_alternatives: Vec::new(),
                    })
                }
            }
            TypeDecl::Resolved { params, defaults } => {
                PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
                    name,
                    surface: PublishedTypeGroupSurface::Template(
                        self.type_resolved
                            .get(group.index())
                            .copied()
                            .flatten()
                            .expect("frozen intrinsic has one final template"),
                    ),
                    parameters: params.clone(),
                    parameter_names: Vec::new(),
                    parameter_defaults: defaults.clone(),
                    conflict_alternatives: Vec::new(),
                })
            }
            TypeDecl::Class {
                class_id,
                params,
                recovery_names,
                recovery_defaults,
                conflict_alternatives,
                param_decl,
                interfaces,
                ..
            } => PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
                name,
                surface: PublishedTypeGroupSurface::Class(*class_id),
                parameters: params.clone(),
                parameter_names: if interfaces.is_empty() {
                    parameter_names(*param_decl)
                } else {
                    recovery_names.clone()
                },
                parameter_defaults: if interfaces.is_empty() {
                    param_decl
                        .iter()
                        .flat_map(|declaration| declaration.params.iter())
                        .map(|parameter| {
                            if parameter.default.is_some() {
                                PublishedTypeParameterDefault::Unsupported
                            } else {
                                PublishedTypeParameterDefault::Absent
                            }
                        })
                        .collect()
                } else {
                    recovery_defaults.clone()
                },
                conflict_alternatives: conflict_alternatives.clone(),
            }),
            TypeDecl::Unavailable { .. } => {
                PublishedTypeGroupTerminal::Unavailable(PublishedTypeGroupUnavailable {
                    cause: TypeGroupUnavailableCause::UnsupportedComposition,
                })
            }
        };
        self.type_group_construction
            .as_mut()
            .expect("type-group construction is consumed exactly once")
            .freeze(group, terminal);
    }

    fn freeze_project_local_unavailable_type_group(&mut self) -> PublishedTypeGroupTerminal {
        PublishedTypeGroupTerminal::Unavailable(PublishedTypeGroupUnavailable {
            cause: TypeGroupUnavailableCause::UnsupportedComposition,
        })
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(in crate::check::checker) fn freeze_seeded_type_groups(&mut self) {
        let groups: Vec<TypeGroupId> = self
            .type_decls
            .changed_entries()
            .into_iter()
            .filter(|(_, declaration)| matches!(declaration, TypeDecl::Resolved { .. }))
            .map(|(index, _)| TypeGroupId(u32::try_from(index).expect("type group index fits u32")))
            .collect();
        for group in groups {
            self.begin_type_group_construction(group);
            self.freeze_type_group(group);
        }
    }

    pub(in crate::check::checker) fn publish_type_groups(&mut self) {
        let alias_terminals = self
            .type_decls
            .changed_entries()
            .into_iter()
            .filter_map(|(_, declaration)| match declaration {
                TypeDecl::Alias {
                    alias_id,
                    alias_template,
                    alias_body,
                    alias_unavailable,
                    params,
                    ..
                } => Some((
                    *alias_id,
                    match (*alias_body, *alias_unavailable) {
                        (Some(body), None) => PublishedAliasTerminal::Ready(
                            typokat_types::alias_semantics::PublishedAliasSurface::new(
                                *alias_id,
                                *alias_template,
                                params.clone(),
                                body,
                            ),
                        ),
                        (None, Some(cause)) => PublishedAliasTerminal::Unavailable(cause),
                        _ => panic!("alias publication requires one final terminal"),
                    },
                )),
                _ => None,
            })
            .collect::<FxHashMap<_, _>>();
        self.staged_published_aliases = Some(
            PublishedAliases::from_terminals(alias_terminals)
                .expect("alias publication preserves exact owner identities"),
        );
        let owned_parameters: Vec<TypeParamId> = self
            .type_decls
            .changed_entries()
            .into_iter()
            .flat_map(|(_, declaration)| match declaration {
                TypeDecl::Interface {
                    recovery_params, ..
                } => recovery_params.as_slice(),
                TypeDecl::Alias { params, .. } => params.as_slice(),
                _ => &[],
            })
            .copied()
            .collect();
        self.interner
            .freeze_type_param_metadata(&owned_parameters)
            .expect("type-group binders freeze once as one validated epoch batch");
        for (_, declaration) in self.type_decls.changed_entries() {
            if let TypeDecl::Class { params, .. } = declaration {
                assert!(params
                    .iter()
                    .all(|param| self.interner.type_param_metadata_is_frozen(*param)));
            }
        }
        let construction = self
            .type_group_construction
            .take()
            .expect("type-group construction is consumed exactly once");
        let unfinished: Vec<_> = construction
            .unfinished_groups()
            .into_iter()
            .map(|(index, state)| {
                let group = TypeGroupId(u32::try_from(index).expect("type group index fits u32"));
                let name = self
                    .binder
                    .type_groups
                    .get(group)
                    .map(|group| group.name.as_str())
                    .unwrap_or("<missing>");
                (index, name, state)
            })
            .collect();
        assert!(
            unfinished.is_empty(),
            "every type group must be explicitly frozen before publication: {unfinished:?}"
        );
        let groups = construction
            .consume(self.type_decls.len())
            .expect("every type group must be explicitly frozen before publication");
        let staged_classes = self
            .staged_published_classes
            .take()
            .expect("class registry must be staged before type publication");
        let staged_aliases = self
            .staged_published_aliases
            .take()
            .expect("alias registry must be staged before type publication");
        self.type_environment
            .publish(groups, staged_classes, staged_aliases);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_drafts() -> ConstructionDrafts<'static> {
        ConstructionDrafts {
            staged_published_classes: None,
            staged_published_aliases: None,
            type_group_construction: None,
            type_decls: Vec::new().into(),
            type_resolved: Vec::new().into(),
            template_fill: super::super::context::TemplateFillTable::new(0, Vec::new()),
        }
    }

    #[test]
    fn empty_registry_has_no_visible_groups() {
        assert!(PublishedTypeGroups::empty().get(TypeGroupId(0)).is_none());
    }

    #[test]
    fn construction_is_not_publishable_while_any_slot_is_unfrozen() {
        let construction = TypeGroupConstruction::new(1);
        assert!(construction.consume(1).is_none());
    }

    #[test]
    #[should_panic(expected = "production type environment is unavailable during construction")]
    fn constructing_environment_rejects_published_queries() {
        let state = TypeEnvironmentState::constructing(empty_drafts());
        let _ = state.published();
    }

    #[test]
    fn publication_physically_consumes_construction_drafts() {
        let mut state = TypeEnvironmentState::constructing(empty_drafts());
        state.publish(
            PublishedTypeGroups::empty(),
            PublishedClasses::empty(),
            PublishedAliases::empty(),
        );

        assert!(state.is_published());
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| state.drafts())).is_err());
    }

    #[test]
    fn project_local_alias_snapshot_is_idempotent_and_invalidates_semantic_identity_once() {
        let alias = AliasId(17);
        let terminal = PublishedAliasTerminal::Ready(
            typokat_types::alias_semantics::PublishedAliasSurface::new(
                alias,
                TypeId(19),
                vec![TypeParamId(23)],
                TypeId(29),
            ),
        );
        let snapshot =
            PublishedAliases::from_terminals(FxHashMap::from_iter([(alias, terminal.clone())]))
                .expect("valid alias snapshot");
        let mut environment = PublishedTypeEnvironment::from_project_local_terminals(Vec::new());
        let initial_identity = Arc::clone(environment.semantic_publication_identity());

        environment
            .validate_project_local_snapshot(&snapshot, &[])
            .expect("disjoint snapshot validates");
        environment
            .install_project_local_alias_snapshot(&snapshot)
            .expect("disjoint snapshot installs");
        assert_eq!(environment.aliases().terminal(alias), Some(&terminal));
        assert!(!Arc::ptr_eq(
            &initial_identity,
            environment.semantic_publication_identity()
        ));

        let installed_identity = Arc::clone(environment.semantic_publication_identity());
        environment
            .install_project_local_alias_snapshot(&snapshot)
            .expect("exact snapshot is idempotent");
        assert!(Arc::ptr_eq(
            &installed_identity,
            environment.semantic_publication_identity()
        ));

        let conflict = PublishedAliases::from_terminals(FxHashMap::from_iter([(
            alias,
            PublishedAliasTerminal::Unavailable(
                typokat_types::alias_semantics::PublishedAliasUnavailable::ComponentFailure,
            ),
        )]))
        .expect("valid conflicting snapshot");
        assert!(environment
            .validate_project_local_snapshot(&conflict, &[])
            .is_err());
        assert_eq!(environment.aliases().terminal(alias), Some(&terminal));
        assert!(Arc::ptr_eq(
            &installed_identity,
            environment.semantic_publication_identity()
        ));
    }

    #[test]
    fn inherited_terminal_preserves_exact_type_and_parameter_identity() {
        let terminal = PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
            name: "Inherited".to_string(),
            surface: PublishedTypeGroupSurface::Template(TypeId(101)),
            parameters: vec![TypeParamId(202)],
            parameter_names: vec!["T".to_string()],
            parameter_defaults: vec![PublishedTypeParameterDefault::Absent],
            conflict_alternatives: Vec::new(),
        });
        let base = published_type_groups_from_terminals(vec![terminal.clone()]);
        let mut construction = TypeGroupConstruction::new(1);
        construction.install_base(&base);

        let inherited = construction.consume(1).expect("installed base is terminal");
        assert_eq!(inherited.get(TypeGroupId(0)), Some(&terminal));
    }
}

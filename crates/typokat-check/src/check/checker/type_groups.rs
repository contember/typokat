use super::context::{
    ConstructionDrafts, Pass, PublishedTypeDecl, TypeDecl, TypeDeclTable, TypeResolvedTable,
};
use super::replay_index::{ReplayClassLookup, ReplayOwner};
use crate::binder::declaration::TypeGroupId;
use crate::check::query::PublishedClassLookup;
use crate::class_semantics::OwnedPublishedClassTerminal;
use crate::class_semantics::{DemandOutcome, PublishedClassSurface, PublishedClasses};
use crate::types::layered::LayeredVec;
use crate::types::repr::{ClassId, TypeParamId};
use crate::types::store::TypeId;
use crate::types::substitute;
use rustc_hash::FxHashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum PublishedTypeGroupSurface {
    Template(TypeId),
    Class(ClassId),
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

    #[cfg(any(test, feature = "test-utils"))]
    pub(in crate::check::checker) fn is_replaced_for_test(&self, group: TypeGroupId) -> bool {
        self.entries
            .changed_iter()
            .any(|(index, _)| index == group.index())
    }
}

/// The only query-visible type environment. Class and named-type registries become
/// visible together through one assignment after both private builders are frozen.
pub(in crate::check::checker) struct PublishedTypeEnvironment {
    classes: PublishedClasses,
    groups: PublishedTypeGroups,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct PublishedTypeEnvironmentProductParts {
    pub(in crate::check::checker) classes: Vec<(ClassId, OwnedPublishedClassTerminal)>,
    pub(in crate::check::checker) groups: Vec<PublishedTypeGroupTerminal>,
}

pub(in crate::check::checker) enum TypeEnvironmentState<'ast> {
    Constructing {
        inherited: Option<PublishedTypeEnvironment>,
        drafts: Option<Box<ConstructionDrafts<'ast>>>,
    },
    Published(PublishedTypeEnvironment),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum TypeGroupPublicationOutcome {
    Ready { publication_validations: usize },
    LibraryIdentitySelectionPending { publication_validations: usize },
}

impl TypeGroupPublicationOutcome {
    pub(in crate::check::checker) fn publication_validations(self) -> usize {
        match self {
            Self::Ready {
                publication_validations,
            }
            | Self::LibraryIdentitySelectionPending {
                publication_validations,
            } => publication_validations,
        }
    }

    pub(in crate::check::checker) fn library_identity_selection_pending(self) -> bool {
        matches!(self, Self::LibraryIdentitySelectionPending { .. })
    }
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

    fn publish(&mut self, groups: PublishedTypeGroups, staged_classes: PublishedClasses) {
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
        let classes = inherited
            .classes
            .extend(staged_classes)
            .expect("class publication epochs must be disjoint");
        *self = Self::Published(PublishedTypeEnvironment { classes, groups });
    }
}

impl PublishedTypeEnvironment {
    pub(in crate::check::checker) fn empty() -> Self {
        Self {
            classes: PublishedClasses::empty(),
            groups: PublishedTypeGroups::empty(),
        }
    }

    #[cfg(test)]
    pub(in crate::check::checker) fn from_explicit_terminals_for_test(
        entries: Vec<PublishedTypeGroupTerminal>,
    ) -> Self {
        Self {
            classes: PublishedClasses::empty(),
            groups: published_type_groups_from_terminals(entries),
        }
    }

    pub(in crate::check::checker) fn classes(&self) -> &PublishedClasses {
        &self.classes
    }

    pub(in crate::check::checker) fn groups(&self) -> &PublishedTypeGroups {
        &self.groups
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
            groups: self.groups.entries.iter().cloned().collect(),
        })
    }

    #[cfg(test)]
    pub(in crate::check::checker) fn from_product_parts(
        parts: PublishedTypeEnvironmentProductParts,
    ) -> Result<Self, &'static str> {
        for terminal in &parts.groups {
            let PublishedTypeGroupTerminal::Ready(group) = terminal else {
                continue;
            };
            if group.parameters.len() != group.parameter_names.len()
                || group.parameters.len() != group.parameter_defaults.len()
            {
                return Err("restored type-group parameter columns have different lengths");
            }
            let mut parameters = std::collections::BTreeSet::new();
            if group
                .parameters
                .iter()
                .any(|parameter| !parameters.insert(*parameter))
            {
                return Err("restored type group repeats a parameter id");
            }
        }
        Ok(Self {
            classes: PublishedClasses::from_owned_terminals(parts.classes)?,
            groups: published_type_groups_from_terminals(parts.groups),
        })
    }

    pub(in crate::check::checker) fn freeze_as_base(&mut self) -> Result<(), &'static str> {
        self.classes.freeze_as_base()?;
        self.groups.entries.freeze_as_base()?;
        self.groups.declarations.freeze_as_base()?;
        self.groups.resolved.freeze_as_base()
    }

    pub(in crate::check::checker) fn fork_delta(&self) -> Result<Self, &'static str> {
        Ok(Self {
            classes: self.classes.fork_delta()?,
            groups: PublishedTypeGroups {
                entries: self.groups.entries.fork_delta()?,
                declarations: self.groups.declarations.fork_delta()?,
                resolved: self.groups.resolved.fork_delta()?,
            },
        })
    }

    pub(in crate::check::checker) fn fork_sparse_delta(&self) -> Result<Self, &'static str> {
        Ok(Self {
            classes: self.classes.fork_sparse_delta()?,
            groups: PublishedTypeGroups {
                entries: self.groups.entries.fork_sparse_delta()?,
                declarations: self.groups.declarations.fork_sparse_delta()?,
                resolved: self.groups.resolved.fork_sparse_delta()?,
            },
        })
    }

    #[cfg(test)]
    pub(in crate::check::checker) fn shares_base_with(&self, other: &Self) -> bool {
        self.classes.shares_base_with(&other.classes)
            && self.groups.entries.shares_base_with(&other.groups.entries)
            && self
                .groups
                .declarations
                .shares_base_with(&other.groups.declarations)
            && self
                .groups
                .resolved
                .shares_base_with(&other.groups.resolved)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(in crate::check::checker) fn base_family_sharing_with(&self, other: &Self) -> [bool; 2] {
        [
            self.groups.entries.shares_base_with(&other.groups.entries)
                && self
                    .groups
                    .declarations
                    .shares_base_with(&other.groups.declarations)
                && self
                    .groups
                    .resolved
                    .shares_base_with(&other.groups.resolved),
            self.classes.shares_base_with(&other.classes),
        ]
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(in crate::check::checker) fn local_family_row_counts_for_test(&self) -> [usize; 2] {
        [
            self.groups.entries.local_len()
                + self.groups.declarations.local_len()
                + self.groups.resolved.local_len(),
            self.classes.local_row_count_for_test(),
        ]
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(in crate::check::checker) fn replacement_family_row_counts_for_test(&self) -> [usize; 2] {
        [
            self.groups.entries.replacement_len()
                + self.groups.declarations.replacement_len()
                + self.groups.resolved.replacement_len(),
            self.classes.replacement_row_count_for_test(),
        ]
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(in crate::check::checker) fn changed_group_terminals(
        &self,
    ) -> impl Iterator<Item = (TypeGroupId, &PublishedTypeGroupTerminal)> {
        self.groups
            .entries
            .changed_iter()
            .filter_map(|(index, terminal)| {
                u32::try_from(index)
                    .ok()
                    .map(|index| (TypeGroupId(index), terminal))
            })
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(in crate::check::checker) fn changed_class_terminals(
        &self,
    ) -> Vec<(ClassId, OwnedPublishedClassTerminal)> {
        self.classes.changed_owned_terminals()
    }
}

#[cfg(test)]
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
        let sparse = base.entries.prefix_overrides_enabled();
        let mut entries = if base.entries.is_sealed() {
            base.entries
                .fork_delta()
                .expect("installed type-group base has no suffix")
        } else {
            let mut entries: LayeredVec<PublishedTypeGroupTerminal> =
                base.entries.iter().cloned().collect::<Vec<_>>().into();
            entries
                .freeze_as_base()
                .expect("source-compiled type-group prefix seals once");
            entries
        };
        if sparse {
            entries.enable_prefix_overrides();
        }
        let (mut declarations, mut resolved) = if base.entries.is_sealed() {
            (
                base.declarations
                    .fork_delta()
                    .expect("installed type declaration base has no suffix"),
                base.resolved
                    .fork_delta()
                    .expect("installed type resolution base has no suffix"),
            )
        } else {
            let mut declarations: LayeredVec<PublishedTypeDecl> =
                base.declarations.iter().cloned().collect::<Vec<_>>().into();
            declarations
                .freeze_as_base()
                .expect("source-compiled declaration prefix seals once");
            let mut resolved: LayeredVec<Option<TypeId>> =
                base.resolved.iter().copied().collect::<Vec<_>>().into();
            resolved
                .freeze_as_base()
                .expect("source-compiled resolution prefix seals once");
            (declarations, resolved)
        };
        if sparse {
            declarations.enable_prefix_overrides();
            resolved.enable_prefix_overrides();
        }
        self.base = Some(PublishedTypeGroups {
            entries,
            declarations,
            resolved,
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

    fn consume(self, expected: usize) -> Option<(PublishedTypeGroups, usize)> {
        if self.expected != expected || self.base_len + self.slots.len() != expected {
            return None;
        }
        let mut publication_validations = 0;
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
            publication_validations += 1;
            let (declaration, resolution) = construction_terminal(&terminal);
            *entries.get_mut_local(index)? = terminal;
            *declarations.get_mut_local(index)? = declaration;
            *resolved.get_mut_local(index)? = resolution;
        }
        for slot in self.slots {
            let terminal = match slot {
                TypeGroupConstructionSlot::Frozen(terminal) => {
                    publication_validations += 1;
                    terminal
                }
                TypeGroupConstructionSlot::Pending | TypeGroupConstructionSlot::Building => {
                    return None
                }
            };
            let (declaration, resolution) = construction_terminal(&terminal);
            entries.push_local(terminal);
            declarations.push_local(declaration);
            resolved.push_local(resolution);
        }
        Some((
            PublishedTypeGroups {
                entries,
                declarations,
                resolved,
            },
            publication_validations,
        ))
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
        let class_lookup = ReplayClassLookup::new(
            self.type_environment.inherited().classes(),
            self.replay_trace.clone(),
        );
        let DemandOutcome::Ready(surface) =
            PublishedClassLookup::published_class(&class_lookup, class)
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
                    .classes
                    .replace_published_surface(replacement)
                    .is_ok()
            });
        if replaced {
            InheritedClassAugmentation::Ready(published)
        } else {
            InheritedClassAugmentation::Unavailable
        }
    }

    pub(in crate::check::checker) fn install_published_type_environment_base(
        &mut self,
        base: PublishedTypeEnvironment,
    ) {
        assert_eq!(
            base.groups.len(),
            usize::try_from(self.binder.prelude_type_group_count)
                .expect("prelude type group count fits usize")
        );
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
                TypeDecl::Class { .. } | TypeDecl::Unavailable { .. } => {
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
            TypeDecl::Interface {
                reserved,
                recovery_params,
                recovery_names,
                recovery_defaults,
                conflict_alternatives,
                ..
            } => {
                if self
                    .private_collision_affected
                    .contains(&ReplayOwner::TypeGroup(group))
                {
                    PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
                        name,
                        surface: PublishedTypeGroupSurface::Template(*reserved),
                        parameters: recovery_params.clone(),
                        parameter_names: recovery_names.clone(),
                        parameter_defaults: recovery_defaults.clone(),
                        conflict_alternatives: conflict_alternatives.clone(),
                    })
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
                params,
                defaults,
                param_decl,
                conditional_template: Some(reserved),
                ..
            }
            | TypeDecl::Alias {
                params,
                defaults,
                param_decl,
                mapped_template: Some(reserved),
                ..
            }
            | TypeDecl::Alias {
                params,
                defaults,
                param_decl,
                object_template: Some(reserved),
                ..
            } => PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
                name,
                surface: PublishedTypeGroupSurface::Template(*reserved),
                parameters: params.clone(),
                parameter_names: parameter_names(*param_decl),
                parameter_defaults: parameter_defaults(*param_decl, defaults),
                conflict_alternatives: Vec::new(),
            }),
            TypeDecl::Alias {
                params,
                defaults,
                param_decl,
                ..
            } => {
                let template = self
                    .type_resolved
                    .get(group.index())
                    .copied()
                    .flatten()
                    .unwrap_or_else(|| panic!("frozen alias {name} has one final template"));
                PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
                    name,
                    surface: PublishedTypeGroupSurface::Template(template),
                    parameters: params.clone(),
                    parameter_names: parameter_names(*param_decl),
                    parameter_defaults: parameter_defaults(*param_decl, defaults),
                    conflict_alternatives: Vec::new(),
                })
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

    pub(in crate::check::checker) fn publish_type_groups(&mut self) -> TypeGroupPublicationOutcome {
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
        let replaced_prefix = !construction.replacements.is_empty();
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
        let (groups, publication_validations) = construction
            .consume(self.type_decls.len())
            .expect("every type group must be explicitly frozen before publication");
        let staged_classes = self
            .staged_published_classes
            .take()
            .expect("class registry must be staged before type publication");
        let mut identity_selection_pending = false;
        let groups = if replaced_prefix {
            let identities_were_installed = self.library_semantic_identities.is_some();
            let preview = PublishedTypeEnvironment {
                classes: self.type_environment.inherited().classes.clone(),
                groups,
            };
            let selected =
                super::library_identities::LibrarySemanticIdentities::select_for_collision_publication(
                    self.binder,
                    &preview,
                    self.interner.store(),
                );
            if selected.all_ready() {
                self.semantic_queries
                    .set_library_object_template(selected.object_template());
                self.library_semantic_identities = Some(selected);
            } else {
                self.semantic_queries.set_library_object_template(None);
                identity_selection_pending = identities_were_installed;
                self.library_semantic_identities = None;
            }
            preview.groups
        } else {
            groups
        };
        self.type_environment.publish(groups, staged_classes);
        if identity_selection_pending {
            TypeGroupPublicationOutcome::LibraryIdentitySelectionPending {
                publication_validations,
            }
        } else {
            TypeGroupPublicationOutcome::Ready {
                publication_validations,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_drafts() -> ConstructionDrafts<'static> {
        ConstructionDrafts {
            staged_published_classes: None,
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
        state.publish(PublishedTypeGroups::empty(), PublishedClasses::empty());

        assert!(state.is_published());
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| state.drafts())).is_err());
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
        let mut base = published_type_groups_from_terminals(vec![terminal.clone()]);
        base.entries
            .freeze_as_base()
            .expect("test base entries seal");
        base.declarations
            .freeze_as_base()
            .expect("test base declarations seal");
        base.resolved
            .freeze_as_base()
            .expect("test base resolutions seal");
        let mut construction = TypeGroupConstruction::new(1);
        construction.install_base(&base);

        let (inherited, publication_validations) =
            construction.consume(1).expect("installed base is terminal");
        assert_eq!(publication_validations, 0);
        assert_eq!(inherited.get(TypeGroupId(0)), Some(&terminal));
    }

    #[test]
    fn published_environment_shares_base_and_constructs_only_a_dense_suffix() {
        let base_terminal =
            PublishedTypeGroupTerminal::Unavailable(PublishedTypeGroupUnavailable {
                cause: TypeGroupUnavailableCause::UnsupportedComposition,
            });
        let mut base =
            PublishedTypeEnvironment::from_explicit_terminals_for_test(vec![base_terminal.clone()]);
        base.freeze_as_base().expect("published base seals");
        let first = base.fork_delta().expect("first published suffix");
        let second = base.fork_delta().expect("second published suffix");
        assert!(first.shares_base_with(&second));

        let mut construction = TypeGroupConstruction::new(3);
        construction.install_base(first.groups());
        for index in 1..3 {
            let group = TypeGroupId(index);
            construction.begin(group);
            construction.freeze(group, base_terminal.clone());
        }
        let (published, validations) = construction.consume(3).expect("dense suffix publishes");
        assert_eq!(validations, 2);
        assert_eq!(published.entries.base_len(), 1);
        assert_eq!(published.entries.local_len(), 2);
        assert_eq!(published.get(TypeGroupId(0)), Some(&base_terminal));
        assert_eq!(second.groups().len(), 1);
    }

    #[test]
    fn sparse_publication_validates_only_replacements_and_dense_suffix_rows() {
        let terminal = PublishedTypeGroupTerminal::Unavailable(PublishedTypeGroupUnavailable {
            cause: TypeGroupUnavailableCause::UnsupportedComposition,
        });
        let mut base =
            PublishedTypeEnvironment::from_explicit_terminals_for_test(vec![terminal.clone(); 64]);
        base.freeze_as_base().expect("published base seals");
        let sparse = base.fork_sparse_delta().expect("sparse publication epoch");
        let mut construction = TypeGroupConstruction::new(65);
        construction.install_base(sparse.groups());
        construction.begin(TypeGroupId(17));
        construction.freeze(TypeGroupId(17), terminal.clone());
        construction.begin(TypeGroupId(64));
        construction.freeze(TypeGroupId(64), terminal);

        let (published, validations) = construction.consume(65).expect("changed rows publish");
        assert_eq!(validations, 2);
        assert_eq!(published.entries.replacement_len(), 1);
        assert_eq!(published.entries.local_len(), 1);
    }
}

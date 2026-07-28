use super::context::{
    ConstructionDrafts, Pass, PublishedTypeDecl, TypeDecl, TypeDeclTable, TypeResolvedTable,
};
use crate::binder::declaration::TypeGroupId;
use crate::class_semantics::OwnedPublishedClassTerminal;
use crate::class_semantics::PublishedClasses;
use crate::types::layered::LayeredVec;
use crate::types::repr::{ClassId, TypeParamId};
use crate::types::store::TypeId;

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
    pub(in crate::check::checker) fn local_group_terminals(
        &self,
    ) -> impl Iterator<Item = (TypeGroupId, &PublishedTypeGroupTerminal)> {
        let base_len = self.groups.entries.base_len();
        self.groups
            .entries
            .local_iter()
            .enumerate()
            .map(move |(index, terminal)| {
                let id = u32::try_from(base_len + index).expect("type group id fits u32");
                (TypeGroupId(id), terminal)
            })
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(in crate::check::checker) fn local_class_terminals(
        &self,
    ) -> Vec<(ClassId, OwnedPublishedClassTerminal)> {
        self.classes.local_owned_terminals()
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
            },
            match group.surface {
                PublishedTypeGroupSurface::Template(template) => Some(template),
                PublishedTypeGroupSurface::Class(_) => None,
            },
        ),
        PublishedTypeGroupTerminal::Unavailable(_) => {
            (PublishedTypeDecl { params: Vec::new() }, None)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TypeGroupConstructionSlot {
    Pending,
    Building,
    Frozen(PublishedTypeGroupTerminal),
}

/// Private, single-use construction authority for one exact type-group epoch.
pub(in crate::check::checker) struct TypeGroupConstruction {
    expected: usize,
    base: Option<PublishedTypeGroups>,
    base_len: usize,
    slots: Vec<TypeGroupConstructionSlot>,
}

impl TypeGroupConstruction {
    pub(in crate::check::checker) fn new(group_count: usize) -> Self {
        Self {
            expected: group_count,
            base: None,
            base_len: 0,
            slots: Vec::new(),
        }
    }

    fn install_base(&mut self, base: &PublishedTypeGroups) {
        assert!(self.base.is_none());
        assert!(base.len() <= self.expected);
        self.base_len = base.len();
        let entries = if base.entries.is_sealed() {
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
        let (declarations, resolved) = if base.entries.is_sealed() {
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
            return false;
        }
        if self.slots.is_empty() {
            return group.index() < self.expected;
        }
        self.slots
            .get(group.index() - self.base_len)
            .is_some_and(|slot| *slot == TypeGroupConstructionSlot::Pending)
    }

    fn is_frozen(&self, group: TypeGroupId) -> bool {
        if group.index() < self.base_len {
            return true;
        }
        self.slots
            .get(group.index() - self.base_len)
            .is_some_and(|slot| matches!(slot, TypeGroupConstructionSlot::Frozen(_)))
    }

    fn consume(self, expected: usize) -> Option<(PublishedTypeGroups, usize)> {
        if self.expected != expected || self.base_len + self.slots.len() != expected {
            return None;
        }
        let mut publication_validations = self.base_len;
        let (mut entries, mut declarations, mut resolved) = match self.base {
            Some(base) => (base.entries, base.declarations, base.resolved),
            None => (
                LayeredVec::default(),
                LayeredVec::default(),
                LayeredVec::default(),
            ),
        };
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
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| match slot {
                TypeGroupConstructionSlot::Pending => Some((self.base_len + index, "pending")),
                TypeGroupConstructionSlot::Building => Some((self.base_len + index, "building")),
                TypeGroupConstructionSlot::Frozen(_) => None,
            })
            .collect()
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
            .is_some_and(|construction| construction.is_pending(group))
    }

    pub(in crate::check::checker) fn type_group_construction_is_frozen(
        &self,
        group: TypeGroupId,
    ) -> bool {
        self.type_group_construction
            .as_ref()
            .is_some_and(|construction| construction.is_frozen(group))
    }

    pub(in crate::check::checker) fn freeze_type_group(&mut self, group: TypeGroupId) {
        let declaration = self
            .type_decls
            .get(group.index())
            .expect("type group draft must exist");
        let name = self
            .binder
            .type_groups
            .get(group)
            .expect("type group metadata must exist")
            .name
            .clone();
        let terminal = match declaration {
            TypeDecl::Interface {
                reserved,
                recovery_params,
                recovery_names,
                recovery_defaults,
                conflict_alternatives,
                ..
            } => PublishedTypeGroupTerminal::Ready(PublishedTypeGroup {
                name,
                surface: PublishedTypeGroupSurface::Template(*reserved),
                parameters: recovery_params.clone(),
                parameter_names: recovery_names.clone(),
                parameter_defaults: recovery_defaults.clone(),
                conflict_alternatives: conflict_alternatives.clone(),
            }),
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
            TypeDecl::Resolved { params } => {
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
                    parameter_defaults: vec![PublishedTypeParameterDefault::Absent; params.len()],
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

    pub(in crate::check::checker) fn freeze_seeded_type_groups(&mut self) {
        let groups: Vec<TypeGroupId> = self
            .type_decls
            .iter()
            .enumerate()
            .map(|(index, declaration)| (self.type_decls.published_len() + index, declaration))
            .filter(|(_, declaration)| matches!(declaration, TypeDecl::Resolved { .. }))
            .map(|(index, _)| TypeGroupId(u32::try_from(index).expect("type group index fits u32")))
            .collect();
        for group in groups {
            self.begin_type_group_construction(group);
            self.freeze_type_group(group);
        }
    }

    pub(in crate::check::checker) fn publish_type_groups(&mut self) -> usize {
        let owned_parameters: Vec<TypeParamId> = self
            .type_decls
            .iter()
            .flat_map(|declaration| match declaration {
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
        for declaration in self.type_decls.iter() {
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
        let (groups, publication_validations) = construction
            .consume(self.type_decls.len())
            .expect("every type group must be explicitly frozen before publication");
        let staged_classes = self
            .staged_published_classes
            .take()
            .expect("class registry must be staged before type publication");
        self.type_environment.publish(groups, staged_classes);
        publication_validations
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
        assert_eq!(publication_validations, 1);
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
        assert_eq!(validations, 3);
        assert_eq!(published.entries.base_len(), 1);
        assert_eq!(published.entries.local_len(), 2);
        assert_eq!(published.get(TypeGroupId(0)), Some(&base_terminal));
        assert_eq!(second.groups().len(), 1);
    }
}

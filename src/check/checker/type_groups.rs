use super::context::{ConstructionDrafts, Pass, TypeDecl};
use crate::binder::declaration::TypeGroupId;
#[cfg(test)]
use crate::class_semantics::PublishedClassSnapshotTerminal;
use crate::class_semantics::PublishedClasses;
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::check::checker) struct PublishedTypeGroups {
    entries: Vec<PublishedTypeGroupTerminal>,
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
#[derive(Clone)]
pub(in crate::check::checker) struct PublishedTypeEnvironment {
    classes: PublishedClasses,
    groups: PublishedTypeGroups,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct PublishedTypeEnvironmentSnapshotParts {
    pub(in crate::check::checker) classes: Vec<(ClassId, PublishedClassSnapshotTerminal)>,
    pub(in crate::check::checker) groups: Vec<PublishedTypeGroupTerminal>,
}

pub(in crate::check::checker) enum TypeEnvironmentState<'ast> {
    Constructing {
        inherited: Option<PublishedTypeEnvironment>,
        drafts: Option<ConstructionDrafts<'ast>>,
    },
    Published(PublishedTypeEnvironment),
}

impl<'ast> TypeEnvironmentState<'ast> {
    pub(in crate::check::checker) fn constructing(drafts: ConstructionDrafts<'ast>) -> Self {
        Self::Constructing {
            inherited: Some(PublishedTypeEnvironment::empty()),
            drafts: Some(drafts),
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
            groups: PublishedTypeGroups { entries },
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
    ) -> (Vec<TypeDecl<'ast>>, Vec<Option<TypeId>>) {
        let mut declarations = Vec::with_capacity(self.groups.entries.len());
        let mut resolved = Vec::with_capacity(self.groups.entries.len());
        for terminal in &self.groups.entries {
            match terminal {
                PublishedTypeGroupTerminal::Ready(group) => {
                    declarations.push(TypeDecl::Resolved {
                        params: group.parameters.clone(),
                    });
                    resolved.push(match group.surface {
                        PublishedTypeGroupSurface::Template(template) => Some(template),
                        PublishedTypeGroupSurface::Class(_) => None,
                    });
                }
                PublishedTypeGroupTerminal::Unavailable(_) => {
                    declarations.push(TypeDecl::Resolved { params: Vec::new() });
                    resolved.push(None);
                }
            }
        }
        (declarations, resolved)
    }

    #[cfg(test)]
    pub(in crate::check::checker) fn snapshot_parts(
        &self,
    ) -> Result<PublishedTypeEnvironmentSnapshotParts, &'static str> {
        Ok(PublishedTypeEnvironmentSnapshotParts {
            classes: self
                .classes
                .snapshot_terminals()
                .ok_or("snapshot class publication contains a non-terminal state")?,
            groups: self.groups.entries.clone(),
        })
    }

    #[cfg(test)]
    pub(in crate::check::checker) fn from_snapshot_parts(
        parts: PublishedTypeEnvironmentSnapshotParts,
    ) -> Result<Self, &'static str> {
        for terminal in &parts.groups {
            let PublishedTypeGroupTerminal::Ready(group) = terminal else {
                continue;
            };
            if group.parameters.len() != group.parameter_names.len()
                || group.parameters.len() != group.parameter_defaults.len()
            {
                return Err("snapshot type-group parameter columns have different lengths");
            }
            let mut parameters = std::collections::BTreeSet::new();
            if group
                .parameters
                .iter()
                .any(|parameter| !parameters.insert(*parameter))
            {
                return Err("snapshot type group repeats a parameter id");
            }
        }
        Ok(Self {
            classes: PublishedClasses::from_snapshot_terminals(parts.classes)?,
            groups: PublishedTypeGroups {
                entries: parts.groups,
            },
        })
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
    slots: Vec<TypeGroupConstructionSlot>,
}

impl TypeGroupConstruction {
    pub(in crate::check::checker) fn new(group_count: usize) -> Self {
        Self {
            slots: vec![TypeGroupConstructionSlot::Pending; group_count],
        }
    }

    fn install_base(&mut self, base: &PublishedTypeGroups) {
        assert!(base.len() <= self.slots.len());
        for (slot, terminal) in self.slots.iter_mut().zip(&base.entries) {
            assert_eq!(*slot, TypeGroupConstructionSlot::Pending);
            *slot = TypeGroupConstructionSlot::Frozen(terminal.clone());
        }
    }

    fn begin(&mut self, group: TypeGroupId) {
        let slot = self
            .slots
            .get_mut(group.index())
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
            .get_mut(group.index())
            .expect("type group construction id must be dense");
        assert_eq!(*slot, TypeGroupConstructionSlot::Building);
        *slot = TypeGroupConstructionSlot::Frozen(terminal);
    }

    fn is_pending(&self, group: TypeGroupId) -> bool {
        self.slots
            .get(group.index())
            .is_some_and(|slot| *slot == TypeGroupConstructionSlot::Pending)
    }

    fn is_frozen(&self, group: TypeGroupId) -> bool {
        self.slots
            .get(group.index())
            .is_some_and(|slot| matches!(slot, TypeGroupConstructionSlot::Frozen(_)))
    }

    fn consume(self, expected: usize) -> Option<(PublishedTypeGroups, usize)> {
        if self.slots.len() != expected {
            return None;
        }
        let mut publication_validations = 0;
        let entries = self
            .slots
            .into_iter()
            .map(|slot| match slot {
                TypeGroupConstructionSlot::Frozen(terminal) => {
                    publication_validations += 1;
                    Some(terminal)
                }
                TypeGroupConstructionSlot::Pending | TypeGroupConstructionSlot::Building => None,
            })
            .collect::<Option<Vec<_>>>()?;
        Some((PublishedTypeGroups { entries }, publication_validations))
    }

    fn unfinished_groups(&self) -> Vec<(usize, &'static str)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| match slot {
                TypeGroupConstructionSlot::Pending => Some((index, "pending")),
                TypeGroupConstructionSlot::Building => Some((index, "building")),
                TypeGroupConstructionSlot::Frozen(_) => None,
            })
            .collect()
    }

    pub(in crate::check::checker) fn for_each_frozen_type_reference(
        &self,
        mut visit: impl FnMut(TypeGroupId, TypeId),
    ) {
        for (index, slot) in self.slots.iter().enumerate() {
            let TypeGroupConstructionSlot::Frozen(PublishedTypeGroupTerminal::Ready(group)) = slot
            else {
                continue;
            };
            let owner = TypeGroupId(u32::try_from(index).expect("type group index fits u32"));
            if let PublishedTypeGroupSurface::Template(template) = group.surface {
                visit(owner, template);
            }
            for default in &group.parameter_defaults {
                if let PublishedTypeParameterDefault::Ready(default) = default {
                    visit(owner, *default);
                }
            }
            for alternative in &group.conflict_alternatives {
                for ty in &alternative.types {
                    visit(owner, *ty);
                }
            }
        }
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
            .filter(|(_, declaration)| matches!(declaration, TypeDecl::Resolved { .. }))
            .map(|(index, _)| TypeGroupId(u32::try_from(index).expect("type group index fits u32")))
            .collect();
        for group in groups {
            self.begin_type_group_construction(group);
            self.freeze_type_group(group);
        }
    }

    pub(in crate::check::checker) fn publish_type_groups(&mut self) -> usize {
        let base_len = self.type_environment.inherited().groups().len();
        let owned_parameters: Vec<TypeParamId> = self
            .type_decls
            .iter()
            .skip(base_len)
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
        for declaration in self.type_decls.iter().skip(base_len) {
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
            type_decls: Vec::new(),
            type_resolved: Vec::new(),
            template_fill: Vec::new(),
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
        let base = PublishedTypeGroups {
            entries: vec![terminal.clone()],
        };
        let mut construction = TypeGroupConstruction::new(1);
        construction.install_base(&base);

        let (inherited, publication_validations) =
            construction.consume(1).expect("installed base is terminal");
        assert_eq!(publication_validations, 1);
        assert_eq!(inherited.get(TypeGroupId(0)), Some(&terminal));
    }
}

//! Binder-authenticated source type-declaration inventory.

use super::super::replay_index::ReplayOwner;
use crate::binder::declaration::{
    DeclId, DeclarationKind, LexicalDeclaration, SourceDeclarationManifest,
    SourceInterfaceOccurrence, SourceTypeFragmentPayload, SourceTypeFragmentPlacement, TypeGroupId,
};
use crate::binder::namespace::SourceUnitKey;
use crate::binder::scope::ScopeId;
use crate::binder::Binder;
use std::collections::BTreeSet;
use std::ops::Range;

#[cfg(any(test, feature = "test-utils"))]
thread_local! {
    static DECLARATION_INVENTORY_WORK_FOR_TEST: std::cell::Cell<DeclarationInventoryWorkForTest> =
        const { std::cell::Cell::new(DeclarationInventoryWorkForTest::ZERO) };
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct DeclarationInventoryWorkForTest {
    pub builds: u64,
    pub source_type_rows: u64,
    pub admitted: u64,
    pub rejected: u64,
    pub exact_join_attempts: u64,
    pub dense_slots: u64,
}

#[cfg(any(test, feature = "test-utils"))]
impl DeclarationInventoryWorkForTest {
    const ZERO: Self = Self {
        builds: 0,
        source_type_rows: 0,
        admitted: 0,
        rejected: 0,
        exact_join_attempts: 0,
        dense_slots: 0,
    };

    fn saturating_sub(self, start: Self) -> Self {
        Self {
            builds: self.builds.saturating_sub(start.builds),
            source_type_rows: self.source_type_rows.saturating_sub(start.source_type_rows),
            admitted: self.admitted.saturating_sub(start.admitted),
            rejected: self.rejected.saturating_sub(start.rejected),
            exact_join_attempts: self
                .exact_join_attempts
                .saturating_sub(start.exact_join_attempts),
            dense_slots: self.dense_slots.saturating_sub(start.dense_slots),
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub struct DeclarationInventoryWorkScopeForTest(DeclarationInventoryWorkForTest);

#[cfg(any(test, feature = "test-utils"))]
impl DeclarationInventoryWorkScopeForTest {
    pub fn start() -> Self {
        Self(DECLARATION_INVENTORY_WORK_FOR_TEST.get())
    }

    pub fn finish(self) -> DeclarationInventoryWorkForTest {
        DECLARATION_INVENTORY_WORK_FOR_TEST
            .get()
            .saturating_sub(self.0)
    }
}

#[derive(Clone, Debug)]
pub(in crate::check::checker) struct BoundTypeDeclaration<'ast> {
    pub declaration: DeclId,
    pub scope: ScopeId,
    pub group: TypeGroupId,
    pub source: SourceUnitKey,
    pub fragment_index: usize,
    pub selected: bool,
    pub payload: SourceTypeFragmentPayload<'ast>,
    pub interface_occurrences: Range<usize>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum SourceTypeFragmentRejection {
    NotSemanticallyAdmitted,
    ClassExpression,
    DefaultCandidateNotAdmitted,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg(test)]
pub(in crate::check::checker) struct RejectedTypeDeclaration {
    pub source_index: usize,
    pub disposition: SourceTypeFragmentRejection,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum SourceTypeDeclarationOutcome {
    Admitted(DeclId),
    Rejected(SourceTypeFragmentRejection),
}

#[derive(Copy, Clone, Debug)]
pub(in crate::check::checker) enum TypeDeclarationInventoryOutcome<'a, 'ast> {
    Admitted(&'a BoundTypeDeclaration<'ast>),
    Rejected {
        disposition: SourceTypeFragmentRejection,
        payload: SourceTypeFragmentPayload<'ast>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum InventoryBuildError {
    SourceMismatch {
        module: ScopeId,
        expected: SourceUnitKey,
        actual: Option<SourceUnitKey>,
    },
    MissingSourceDeclaration {
        kind: DeclarationKind,
        binding_start: u32,
    },
    AttachedDeclarationMissingGroup(DeclId),
    MissingGroup(TypeGroupId),
    FragmentMultiplicity {
        declaration: DeclId,
        group: TypeGroupId,
        matches: usize,
    },
    FragmentIdentityMismatch {
        declaration: DeclId,
        group: TypeGroupId,
    },
    ExactSiteMismatch {
        declaration: DeclId,
        expected_kind: DeclarationKind,
        actual_kind: DeclarationKind,
        expected_module: ScopeId,
        actual_module: ScopeId,
        expected_declaration_span: crate::span::Span,
        actual_declaration_span: crate::span::Span,
        expected_binding_span: crate::span::Span,
        actual_binding_span: crate::span::Span,
    },
    UnexpectedSemanticAdmission {
        declaration: DeclId,
        placement: SourceTypeFragmentPlacement,
    },
    DuplicatePayload(DeclId),
    AuthenticatedFragmentMissingPayload {
        declaration: DeclId,
        group: TypeGroupId,
    },
}

impl std::fmt::Display for InventoryBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "canonical declaration inventory mismatch: {self:?}"
        )
    }
}

/// Module-local offset-dense rows keyed by canonical [`DeclId`].
#[derive(Debug)]
pub(in crate::check::checker) struct DenseDeclMap<'ast> {
    base: Option<DeclId>,
    slots: Box<[Option<BoundTypeDeclaration<'ast>>]>,
}

impl<'ast> DenseDeclMap<'ast> {
    fn from_rows(
        rows: Vec<(DeclId, BoundTypeDeclaration<'ast>)>,
    ) -> Result<Self, InventoryBuildError> {
        let Some(base) = rows.iter().map(|(declaration, _)| declaration.0).min() else {
            return Ok(Self {
                base: None,
                slots: Box::new([]),
            });
        };
        let max = rows
            .iter()
            .map(|(declaration, _)| declaration.0)
            .max()
            .expect("non-empty declaration rows have a maximum");
        let len = usize::try_from(max.saturating_sub(base).saturating_add(1))
            .expect("module-local declaration span fits usize");
        let mut slots = vec![None; len].into_boxed_slice();
        for (declaration, row) in rows {
            let offset = usize::try_from(declaration.0.saturating_sub(base))
                .expect("module-local declaration offset fits usize");
            if slots[offset].replace(row).is_some() {
                return Err(InventoryBuildError::DuplicatePayload(declaration));
            }
        }
        Ok(Self {
            base: Some(DeclId(base)),
            slots,
        })
    }

    pub fn get(&self, declaration: DeclId) -> Option<&BoundTypeDeclaration<'ast>> {
        let base = self.base?;
        let offset = declaration.0.checked_sub(base.0)?;
        self.slots
            .get(usize::try_from(offset).ok()?)
            .and_then(Option::as_ref)
    }

    pub fn base(&self) -> Option<DeclId> {
        self.base
    }

    pub fn slot_len(&self) -> usize {
        self.slots.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &BoundTypeDeclaration<'ast>> {
        self.slots.iter().filter_map(Option::as_ref)
    }
}

/// One canonical source manifest joined to one semantically bound module.
#[derive(Debug)]
pub(in crate::check::checker) struct BoundTypeDeclarationInventory<'ast> {
    module: ScopeId,
    source: SourceUnitKey,
    manifest: SourceDeclarationManifest<'ast>,
    declarations: DenseDeclMap<'ast>,
    outcomes: Box<[SourceTypeDeclarationOutcome]>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct DeclarationInventoryReceipt {
    pub module: ScopeId,
    pub source: SourceUnitKey,
    pub base: Option<DeclId>,
    pub slots: usize,
    pub admitted: usize,
    pub rejected: usize,
    pub selected: usize,
    pub interface_occurrences: usize,
}

impl<'ast> BoundTypeDeclarationInventory<'ast> {
    pub fn build(
        binder: &Binder,
        module: ScopeId,
        source_key: SourceUnitKey,
        manifest: SourceDeclarationManifest<'ast>,
        selected: Option<&BTreeSet<ReplayOwner>>,
    ) -> Result<Self, InventoryBuildError> {
        #[cfg(any(test, feature = "test-utils"))]
        let source_type_rows = manifest.type_fragments().len();
        let result = Self::build_with_exact_lookup(
            binder,
            module,
            source_key,
            manifest,
            selected,
            |module, binding_start, kind| binder.exact_declaration_at(module, binding_start, kind),
        );
        #[cfg(any(test, feature = "test-utils"))]
        DECLARATION_INVENTORY_WORK_FOR_TEST.with(|cell| {
            let work = cell.get();
            let (admitted, rejected, exact_join_attempts, dense_slots) =
                result.as_ref().map_or((0, 0, 0, 0), |inventory| {
                    (
                        inventory.declarations.iter().count(),
                        inventory.rejected_count(),
                        source_type_rows,
                        inventory.declarations.slot_len(),
                    )
                });
            cell.set(DeclarationInventoryWorkForTest {
                builds: work.builds.saturating_add(1),
                source_type_rows: work
                    .source_type_rows
                    .saturating_add(u64::try_from(source_type_rows).unwrap_or(u64::MAX)),
                admitted: work
                    .admitted
                    .saturating_add(u64::try_from(admitted).unwrap_or(u64::MAX)),
                rejected: work
                    .rejected
                    .saturating_add(u64::try_from(rejected).unwrap_or(u64::MAX)),
                exact_join_attempts: work
                    .exact_join_attempts
                    .saturating_add(u64::try_from(exact_join_attempts).unwrap_or(u64::MAX)),
                dense_slots: work
                    .dense_slots
                    .saturating_add(u64::try_from(dense_slots).unwrap_or(u64::MAX)),
            });
        });
        result
    }

    fn build_with_exact_lookup<'binder>(
        binder: &'binder Binder,
        module: ScopeId,
        source_key: SourceUnitKey,
        manifest: SourceDeclarationManifest<'ast>,
        selected: Option<&BTreeSet<ReplayOwner>>,
        mut exact_declaration_at: impl FnMut(
            ScopeId,
            u32,
            DeclarationKind,
        ) -> Option<&'binder LexicalDeclaration>,
    ) -> Result<Self, InventoryBuildError> {
        let actual_source = binder.source_for_module(module);
        if actual_source != Some(source_key) {
            return Err(InventoryBuildError::SourceMismatch {
                module,
                expected: source_key,
                actual: actual_source,
            });
        }
        let mut rows = Vec::new();
        let mut outcomes = Vec::with_capacity(manifest.type_fragments().len());

        for source in manifest.type_fragments() {
            let exact = exact_declaration_at(module, source.binding_span.start, source.kind);
            let recorded = binder.declarations.declaration_at_site(
                module,
                source.binding_span.start,
                source.kind,
            );
            let matches_source = |declaration: &LexicalDeclaration| {
                declaration.kind == source.kind
                    && declaration.site.module == module
                    && declaration.site.declaration_span == source.declaration_span
                    && declaration.site.binding_span == source.binding_span
            };
            let global_has_issues = match source.placement {
                SourceTypeFragmentPlacement::GlobalAugmentationBody { global_start } => {
                    binder.global_augmentation_has_issues(module, global_start)
                }
                _ => None,
            };
            let disposition = match source.placement {
                SourceTypeFragmentPlacement::ClassExpression => {
                    Some(SourceTypeFragmentRejection::ClassExpression)
                }
                SourceTypeFragmentPlacement::DefaultClass
                    if exact.is_none_or(|declaration| declaration.type_group.is_none()) =>
                {
                    Some(SourceTypeFragmentRejection::DefaultCandidateNotAdmitted)
                }
                SourceTypeFragmentPlacement::UnadmittedAmbientExternalModule
                    if exact.is_none()
                        && recorded.is_some_and(|declaration| {
                            declaration.site.scope.is_none()
                                && declaration.type_group.is_none()
                                && declaration.kind == source.kind
                                && declaration.site.module == module
                                && declaration.site.declaration_span == source.declaration_span
                                && declaration.site.binding_span == source.binding_span
                        }) =>
                {
                    Some(SourceTypeFragmentRejection::NotSemanticallyAdmitted)
                }
                SourceTypeFragmentPlacement::GlobalAugmentationBody { .. }
                    if global_has_issues == Some(true)
                        && match exact {
                            Some(declaration) => {
                                matches_source(declaration) && declaration.type_group.is_none()
                            }
                            None => recorded.is_some_and(|declaration| {
                                matches_source(declaration)
                                    && declaration.site.scope.is_none()
                                    && declaration.type_group.is_none()
                            }),
                        } =>
                {
                    Some(SourceTypeFragmentRejection::NotSemanticallyAdmitted)
                }
                _ => None,
            };
            if let Some(disposition) = disposition {
                if let Some((declaration, group)) = exact.and_then(|declaration| {
                    declaration.type_group.map(|group| (declaration, group))
                }) {
                    return Err(InventoryBuildError::FragmentIdentityMismatch {
                        declaration: declaration.id,
                        group,
                    });
                }
                outcomes.push(SourceTypeDeclarationOutcome::Rejected(disposition));
                continue;
            }
            let Some(exact) = exact else {
                return Err(InventoryBuildError::MissingSourceDeclaration {
                    kind: source.kind,
                    binding_start: source.binding_span.start,
                });
            };
            if exact.kind != source.kind
                || exact.site.module != module
                || exact.site.declaration_span != source.declaration_span
                || exact.site.binding_span != source.binding_span
            {
                return Err(InventoryBuildError::ExactSiteMismatch {
                    declaration: exact.id,
                    expected_kind: source.kind,
                    actual_kind: exact.kind,
                    expected_module: module,
                    actual_module: exact.site.module,
                    expected_declaration_span: source.declaration_span,
                    actual_declaration_span: exact.site.declaration_span,
                    expected_binding_span: source.binding_span,
                    actual_binding_span: exact.site.binding_span,
                });
            }
            if source.placement == SourceTypeFragmentPlacement::UnadmittedAmbientExternalModule {
                return Err(InventoryBuildError::UnexpectedSemanticAdmission {
                    declaration: exact.id,
                    placement: source.placement,
                });
            }
            let Some(group) = exact.type_group else {
                return Err(InventoryBuildError::AttachedDeclarationMissingGroup(
                    exact.id,
                ));
            };
            let bound = binder
                .type_groups
                .get(group)
                .ok_or(InventoryBuildError::MissingGroup(group))?;
            let mut matches = bound
                .fragments
                .iter()
                .enumerate()
                .filter(|(_, fragment)| fragment.declaration == exact.id);
            let Some((fragment_index, fragment)) = matches.next() else {
                return Err(InventoryBuildError::FragmentMultiplicity {
                    declaration: exact.id,
                    group,
                    matches: 0,
                });
            };
            if matches.next().is_some() {
                return Err(InventoryBuildError::FragmentMultiplicity {
                    declaration: exact.id,
                    group,
                    matches: 2,
                });
            }
            let scope = exact
                .site
                .scope
                .ok_or(InventoryBuildError::FragmentIdentityMismatch {
                    declaration: exact.id,
                    group,
                })?;
            if fragment.kind != source.payload.kind()
                || fragment.scope != scope
                || fragment.site != exact.site
                || fragment.source != source_key
            {
                return Err(InventoryBuildError::FragmentIdentityMismatch {
                    declaration: exact.id,
                    group,
                });
            }
            rows.push((
                exact.id,
                BoundTypeDeclaration {
                    declaration: exact.id,
                    scope,
                    group,
                    source: fragment.source,
                    fragment_index,
                    selected: selected
                        .is_none_or(|owners| owners.contains(&ReplayOwner::TypeGroup(group))),
                    payload: source.payload,
                    interface_occurrences: source.interface_occurrences.clone(),
                },
            ));
            outcomes.push(SourceTypeDeclarationOutcome::Admitted(exact.id));
        }

        let declarations = DenseDeclMap::from_rows(rows)?;
        for occurrence in manifest.declarations().iter().filter(|occurrence| {
            matches!(
                occurrence.kind,
                DeclarationKind::TypeAlias
                    | DeclarationKind::Interface
                    | DeclarationKind::Class
                    | DeclarationKind::Enum
            )
        }) {
            let Some(exact) =
                exact_declaration_at(module, occurrence.binding_span.start, occurrence.kind)
            else {
                continue;
            };
            let Some(group) = exact.type_group else {
                continue;
            };
            if declarations.get(exact.id).is_none() {
                return Err(InventoryBuildError::AuthenticatedFragmentMissingPayload {
                    declaration: exact.id,
                    group,
                });
            }
        }
        if let Some(selected) = selected {
            for owner in selected {
                let ReplayOwner::TypeGroup(group) = owner else {
                    continue;
                };
                let Some(bound) = binder.type_groups.get(*group) else {
                    return Err(InventoryBuildError::MissingGroup(*group));
                };
                for fragment in &bound.fragments {
                    if fragment.site.module == module
                        && declarations.get(fragment.declaration).is_none()
                    {
                        return Err(InventoryBuildError::AuthenticatedFragmentMissingPayload {
                            declaration: fragment.declaration,
                            group: *group,
                        });
                    }
                }
            }
        }

        Ok(Self {
            module,
            source: source_key,
            manifest,
            declarations,
            outcomes: outcomes.into_boxed_slice(),
        })
    }

    pub fn declarations(&self) -> &DenseDeclMap<'ast> {
        &self.declarations
    }

    #[cfg(test)]
    pub fn rejected(&self) -> Vec<RejectedTypeDeclaration> {
        self.outcomes
            .iter()
            .enumerate()
            .filter_map(|(source_index, outcome)| match outcome {
                SourceTypeDeclarationOutcome::Admitted(_) => None,
                SourceTypeDeclarationOutcome::Rejected(disposition) => {
                    Some(RejectedTypeDeclaration {
                        source_index,
                        disposition: *disposition,
                    })
                }
            })
            .collect()
    }

    fn rejected_count(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|outcome| matches!(outcome, SourceTypeDeclarationOutcome::Rejected(_)))
            .count()
    }

    pub fn interface_occurrences(
        &self,
        declaration: DeclId,
    ) -> Option<&[SourceInterfaceOccurrence]> {
        let row = self.declarations.get(declaration)?;
        Some(
            self.manifest
                .interface_occurrences(row.interface_occurrences.clone()),
        )
    }

    pub fn module(&self) -> ScopeId {
        self.module
    }

    /// Iterate source candidates in source order without rebuilding an AST index.
    pub fn outcomes(&self) -> impl Iterator<Item = TypeDeclarationInventoryOutcome<'_, 'ast>> {
        debug_assert_eq!(self.manifest.type_fragments().len(), self.outcomes.len());
        self.manifest
            .type_fragments()
            .iter()
            .zip(self.outcomes.iter())
            .map(|(source, outcome)| match outcome {
                SourceTypeDeclarationOutcome::Admitted(declaration) => {
                    TypeDeclarationInventoryOutcome::Admitted(
                        self.declarations
                            .get(*declaration)
                            .expect("admitted source outcome has one dense declaration row"),
                    )
                }
                SourceTypeDeclarationOutcome::Rejected(disposition) => {
                    TypeDeclarationInventoryOutcome::Rejected {
                        disposition: *disposition,
                        payload: source.payload,
                    }
                }
            })
    }

    /// Read every shadow row without influencing declaration construction.
    pub fn receipt(&self) -> DeclarationInventoryReceipt {
        let mut admitted = 0usize;
        let mut selected = 0usize;
        let mut interface_occurrences = 0usize;
        for row in self.declarations.iter() {
            debug_assert_eq!(row.source, self.source);
            let _ = (
                row.declaration,
                row.scope,
                row.group,
                row.fragment_index,
                row.payload.kind(),
            );
            admitted = admitted.saturating_add(1);
            selected = selected.saturating_add(usize::from(row.selected));
            interface_occurrences = interface_occurrences.saturating_add(
                self.interface_occurrences(row.declaration)
                    .map_or(0, <[SourceInterfaceOccurrence]>::len),
            );
        }
        DeclarationInventoryReceipt {
            module: self.module,
            source: self.source,
            base: self.declarations.base(),
            slots: self.declarations.slot_len(),
            admitted,
            rejected: self.rejected_count(),
            selected,
            interface_occurrences,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binder::bind::ProjectBinderBuilder;
    use crate::binder::declaration::DeclarationKind;
    use crate::binder::namespace::{CompilationUnit, SourceUnitKey};
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    #[test]
    fn inventory_is_decl_id_dense_in_canonical_source_order() {
        let prelude_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
        let allocator = Allocator::default();
        let source = "interface Merged { first: string } interface Merged { second: number } { interface Local { value: string } }";
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let prelude_manifest = SourceDeclarationManifest::build(&prelude.program);
        let manifest = SourceDeclarationManifest::build(&parsed.program);
        let mut builder = ProjectBinderBuilder::new_with_declaration_manifest(
            &prelude.program,
            &prelude_manifest,
        );
        let unit = CompilationUnit::implementation(SourceUnitKey::SINGLE_SOURCE, &parsed.program);
        let (module, _) =
            builder.add_module_with_declaration_manifest(&parsed.program, &manifest, &[], unit);
        let binder = builder.finish(module);
        let inventory = BoundTypeDeclarationInventory::build(
            &binder,
            module,
            SourceUnitKey::SINGLE_SOURCE,
            manifest,
            None,
        )
        .expect("bound source has one canonical inventory");

        assert_eq!(inventory.declarations().iter().count(), 3);
        assert_eq!(inventory.declarations().slot_len(), 3);
        assert_eq!(inventory.declarations().base(), Some(DeclId(0)));
        let merged = inventory
            .declarations()
            .iter()
            .filter(|row| {
                binder
                    .type_groups
                    .get(row.group)
                    .is_some_and(|group| group.name == "Merged")
            })
            .collect::<Vec<_>>();
        assert_eq!(
            merged
                .iter()
                .map(|row| row.fragment_index)
                .collect::<Vec<_>>(),
            [0, 1]
        );

        assert_eq!(
            inventory
                .outcomes()
                .map(|outcome| match outcome {
                    TypeDeclarationInventoryOutcome::Admitted(row) => row.declaration,
                    TypeDeclarationInventoryOutcome::Rejected { .. } => {
                        panic!("fixture contains no rejected declaration")
                    }
                })
                .collect::<Vec<_>>(),
            [DeclId(0), DeclId(1), DeclId(2)]
        );
    }

    #[test]
    fn inventory_rejects_unadmitted_class_expression_and_default_candidates() {
        for (source, expected) in [
            (
                "const Value = class NamedExpression {};",
                SourceTypeFragmentRejection::ClassExpression,
            ),
            (
                "export default class NamedDefault {}",
                SourceTypeFragmentRejection::DefaultCandidateNotAdmitted,
            ),
            (
                "export default class {}",
                SourceTypeFragmentRejection::DefaultCandidateNotAdmitted,
            ),
        ] {
            let prelude_allocator = Allocator::default();
            let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
            let allocator = Allocator::default();
            let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let prelude_manifest = SourceDeclarationManifest::build(&prelude.program);
            let manifest = SourceDeclarationManifest::build(&parsed.program);
            let mut builder = ProjectBinderBuilder::new_with_declaration_manifest(
                &prelude.program,
                &prelude_manifest,
            );
            let unit =
                CompilationUnit::implementation(SourceUnitKey::SINGLE_SOURCE, &parsed.program);
            let (module, _) =
                builder.add_module_with_declaration_manifest(&parsed.program, &manifest, &[], unit);
            let binder = builder.finish(module);
            let inventory = BoundTypeDeclarationInventory::build(
                &binder,
                module,
                SourceUnitKey::SINGLE_SOURCE,
                manifest,
                None,
            )
            .expect("unadmitted class syntax is an explicit rejection");
            assert!(inventory.declarations().iter().next().is_none());
            assert_eq!(inventory.rejected().len(), 1);
            assert_eq!(inventory.rejected()[0].disposition, expected);
        }
    }

    #[test]
    fn ordinary_missing_or_attached_without_group_is_hard() {
        let prelude_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
        let allocator = Allocator::default();
        let source = "type Alias = string;";
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let prelude_manifest = SourceDeclarationManifest::build(&prelude.program);
        let manifest = SourceDeclarationManifest::build(&parsed.program);
        let mut builder = ProjectBinderBuilder::new_with_declaration_manifest(
            &prelude.program,
            &prelude_manifest,
        );
        let unit = CompilationUnit::implementation(SourceUnitKey::SINGLE_SOURCE, &parsed.program);
        let (module, _) =
            builder.add_module_with_declaration_manifest(&parsed.program, &manifest, &[], unit);
        let mut binder = builder.finish(module);

        let missing_manifest = SourceDeclarationManifest::build(&parsed.program);
        let missing = BoundTypeDeclarationInventory::build_with_exact_lookup(
            &binder,
            module,
            SourceUnitKey::SINGLE_SOURCE,
            missing_manifest,
            None,
            |_, _, _| None,
        )
        .expect_err("an ordinary source fragment without an exact declaration is a hard error");
        assert!(matches!(
            missing,
            InventoryBuildError::MissingSourceDeclaration {
                kind: DeclarationKind::TypeAlias,
                ..
            }
        ));

        let alias_start = u32::try_from(source.find("Alias").expect("Alias binding offset"))
            .expect("source offset fits u32");
        let alias = binder
            .exact_declaration_at(module, alias_start, DeclarationKind::TypeAlias)
            .expect("Alias declaration is attached")
            .id;
        binder
            .declarations
            .clear_type_group_for_inventory_test(alias);
        let missing_group_manifest = SourceDeclarationManifest::build(&parsed.program);
        let missing_group = BoundTypeDeclarationInventory::build(
            &binder,
            module,
            SourceUnitKey::SINGLE_SOURCE,
            missing_group_manifest,
            None,
        )
        .expect_err("an attached ordinary control without a type group is a hard error");
        assert_eq!(
            missing_group,
            InventoryBuildError::AttachedDeclarationMissingGroup(alias)
        );

        binder
            .declarations
            .clear_semantic_attachment_for_inventory_test(alias);
        let missing_attachment_manifest = SourceDeclarationManifest::build(&parsed.program);
        let missing_attachment = BoundTypeDeclarationInventory::build(
            &binder,
            module,
            SourceUnitKey::SINGLE_SOURCE,
            missing_attachment_manifest,
            None,
        )
        .expect_err("an ordinary raw row without semantic attachment is a hard error");
        assert!(matches!(
            missing_attachment,
            InventoryBuildError::MissingSourceDeclaration {
                kind: DeclarationKind::TypeAlias,
                ..
            }
        ));
    }

    #[test]
    fn exact_join_rejects_a_same_kind_row_from_different_spans() {
        let prelude_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
        let allocator = Allocator::default();
        let source = "type First = string; type Second = number;";
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let prelude_manifest = SourceDeclarationManifest::build(&prelude.program);
        let manifest = SourceDeclarationManifest::build(&parsed.program);
        let mut builder = ProjectBinderBuilder::new_with_declaration_manifest(
            &prelude.program,
            &prelude_manifest,
        );
        let unit = CompilationUnit::implementation(SourceUnitKey::SINGLE_SOURCE, &parsed.program);
        let (module, _) =
            builder.add_module_with_declaration_manifest(&parsed.program, &manifest, &[], unit);
        let binder = builder.finish(module);
        let first_start = u32::try_from(source.find("First").expect("First binding offset"))
            .expect("source offset fits u32");
        let second_start = u32::try_from(source.find("Second").expect("Second binding offset"))
            .expect("source offset fits u32");
        let second = binder
            .exact_declaration_at(module, second_start, DeclarationKind::TypeAlias)
            .expect("Second alias declaration");

        let error = BoundTypeDeclarationInventory::build_with_exact_lookup(
            &binder,
            module,
            SourceUnitKey::SINGLE_SOURCE,
            manifest,
            None,
            |lookup_module, binding_start, kind| {
                if binding_start == first_start && kind == DeclarationKind::TypeAlias {
                    Some(second)
                } else {
                    binder.exact_declaration_at(lookup_module, binding_start, kind)
                }
            },
        )
        .expect_err("an exact row with different source spans is rejected");
        assert!(matches!(
            error,
            InventoryBuildError::ExactSiteMismatch {
                declaration,
                expected_kind: DeclarationKind::TypeAlias,
                actual_kind: DeclarationKind::TypeAlias,
                expected_module,
                actual_module,
                ..
            } if declaration == second.id
                && expected_module == module
                && actual_module == module
        ));
    }

    #[test]
    fn inventory_rejects_only_authenticated_invalid_global_body_rows() {
        fn assert_invalid(source: &str, clear_attachment: bool) {
            let prelude_allocator = Allocator::default();
            let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
            let allocator = Allocator::default();
            let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let prelude_manifest = SourceDeclarationManifest::build(&prelude.program);
            let manifest = SourceDeclarationManifest::build(&parsed.program);
            assert!(manifest.type_fragments().iter().all(|fragment| matches!(
                fragment.placement,
                SourceTypeFragmentPlacement::GlobalAugmentationBody { .. }
            )));
            let mut builder = ProjectBinderBuilder::new_with_declaration_manifest(
                &prelude.program,
                &prelude_manifest,
            );
            let unit =
                CompilationUnit::implementation(SourceUnitKey::SINGLE_SOURCE, &parsed.program);
            let (module, _) =
                builder.add_module_with_declaration_manifest(&parsed.program, &manifest, &[], unit);
            let mut binder = builder.finish(module);
            let global_start = match manifest.type_fragments()[0].placement {
                SourceTypeFragmentPlacement::GlobalAugmentationBody { global_start } => {
                    global_start
                }
                placement => panic!("unexpected placement: {placement:?}"),
            };
            assert_eq!(
                binder.global_augmentation_has_issues(module, global_start),
                Some(true)
            );
            assert_eq!(
                binder.global_augmentation_has_issues(module, u32::MAX),
                None
            );
            if clear_attachment {
                let fragment = &manifest.type_fragments()[0];
                let declaration = binder
                    .declarations
                    .declaration_at_site(module, fragment.binding_span.start, fragment.kind)
                    .expect("global body declaration is preseeded")
                    .id;
                binder
                    .declarations
                    .clear_semantic_attachment_for_inventory_test(declaration);
            }
            let inventory = BoundTypeDeclarationInventory::build(
                &binder,
                module,
                SourceUnitKey::SINGLE_SOURCE,
                manifest,
                None,
            )
            .expect("invalid global body rows are explicit rejections");
            assert!(inventory.declarations().iter().next().is_none());
            assert_eq!(inventory.rejected().len(), 1);
            assert_eq!(
                inventory.rejected()[0].disposition,
                SourceTypeFragmentRejection::NotSemanticallyAdmitted
            );
        }

        assert_invalid("declare global { interface InvalidScriptGlobal {} }", false);
        assert_invalid(
            "export {}; global { interface InvalidUndeclaredGlobal {} }",
            false,
        );
        assert_invalid(
            "export {}; global { interface RawUnattachedGlobal {} }",
            true,
        );
    }

    #[test]
    fn inventory_admits_legal_and_grouped_descendant_global_body_rows() {
        for source in [
            "export {}; declare global { interface LegalGlobal {} }",
            "export {}; global { namespace Private { interface GroupedDescendant {} } }",
        ] {
            let prelude_allocator = Allocator::default();
            let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
            let allocator = Allocator::default();
            let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let prelude_manifest = SourceDeclarationManifest::build(&prelude.program);
            let manifest = SourceDeclarationManifest::build(&parsed.program);
            let [fragment] = manifest.type_fragments() else {
                panic!("one global-body type fragment")
            };
            let SourceTypeFragmentPlacement::GlobalAugmentationBody { global_start } =
                fragment.placement
            else {
                panic!("global body placement")
            };
            let mut builder = ProjectBinderBuilder::new_with_declaration_manifest(
                &prelude.program,
                &prelude_manifest,
            );
            let unit =
                CompilationUnit::implementation(SourceUnitKey::SINGLE_SOURCE, &parsed.program);
            let (module, _) =
                builder.add_module_with_declaration_manifest(&parsed.program, &manifest, &[], unit);
            let binder = builder.finish(module);
            assert_eq!(
                binder.global_augmentation_has_issues(module, global_start),
                Some(source.contains("GroupedDescendant"))
            );
            let inventory = BoundTypeDeclarationInventory::build(
                &binder,
                module,
                SourceUnitKey::SINGLE_SOURCE,
                manifest,
                None,
            )
            .expect("grouped global body rows remain admitted");
            assert_eq!(inventory.declarations().iter().count(), 1);
            assert!(inventory.rejected().is_empty());
        }
    }

    #[test]
    fn frozen_continuation_rejects_invalid_global_body_rows() {
        let prelude_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
        let library_allocator = Allocator::default();
        let library = Parser::new(
            &library_allocator,
            "interface FrozenGlobalInventoryMarker {}",
            SourceType::d_ts(),
        )
        .parse();
        let prelude_manifest = SourceDeclarationManifest::build(&prelude.program);
        let library_manifest = SourceDeclarationManifest::build(&library.program);
        let mut builder = ProjectBinderBuilder::new_with_declaration_manifest(
            &prelude.program,
            &prelude_manifest,
        );
        let library_unit = CompilationUnit::library(
            SourceUnitKey(1),
            crate::source::LibraryFileOrdinal::new(0),
            &library.program,
        );
        let [library_module] = builder
            .try_add_library_modules_with_declaration_manifests(&[(
                &library.program,
                &library_manifest,
                library_unit,
            )])
            .expect("library base binds")
            .try_into()
            .expect("one library module");
        let mut base = builder.finish(library_module);
        base.freeze_as_base().expect("library base freezes");

        let allocator = Allocator::default();
        let parsed = Parser::new(
            &allocator,
            "export {}; global { interface FrozenInvalidGlobal {} }",
            SourceType::ts(),
        )
        .parse();
        let manifest = SourceDeclarationManifest::build(&parsed.program);
        let (mut builder, source) = ProjectBinderBuilder::resume_frozen_library(base);
        let unit = CompilationUnit::implementation(source, &parsed.program);
        builder.reserve_script_namespace_roots([(&parsed.program, unit)]);
        let (module, _) =
            builder.add_module_with_declaration_manifest(&parsed.program, &manifest, &[], unit);
        let binder = builder
            .finish_frozen_library_continuation(Some(module))
            .expect("continuation binder finishes");
        let inventory =
            BoundTypeDeclarationInventory::build(&binder, module, source, manifest, None)
                .expect("invalid continuation global body is rejected explicitly");
        assert!(inventory.declarations().iter().next().is_none());
        assert_eq!(inventory.rejected().len(), 1);
    }

    #[test]
    fn inventory_rejects_types_inside_unadmitted_ambient_external_modules() {
        let prelude_allocator = Allocator::default();
        let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
        let allocator = Allocator::default();
        let source = r#"declare module "external" { export interface Payload { value: string } }"#;
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let prelude_manifest = SourceDeclarationManifest::build(&prelude.program);
        let manifest = SourceDeclarationManifest::build(&parsed.program);
        assert!(matches!(
            manifest.type_fragments(),
            [fragment]
                if fragment.placement
                    == SourceTypeFragmentPlacement::UnadmittedAmbientExternalModule
        ));
        let mut builder = ProjectBinderBuilder::new_with_declaration_manifest(
            &prelude.program,
            &prelude_manifest,
        );
        let unit = CompilationUnit::implementation(SourceUnitKey::SINGLE_SOURCE, &parsed.program);
        let (module, _) =
            builder.add_module_with_declaration_manifest(&parsed.program, &manifest, &[], unit);
        let binder = builder.finish(module);
        let inventory = BoundTypeDeclarationInventory::build(
            &binder,
            module,
            SourceUnitKey::SINGLE_SOURCE,
            manifest,
            None,
        )
        .expect("unsupported ambient-module contents are explicit rejections");

        assert!(inventory.declarations().iter().next().is_none());
        assert_eq!(inventory.rejected().len(), 1);
        assert_eq!(
            inventory.rejected()[0].disposition,
            SourceTypeFragmentRejection::NotSemanticallyAdmitted
        );

        let allocator = Allocator::default();
        let source = r#"
            type Attached = string;
            declare module "external" { export interface Payload { value: string } }
        "#;
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let manifest = SourceDeclarationManifest::build(&parsed.program);
        let ambient = manifest
            .type_fragments()
            .iter()
            .find(|fragment| {
                fragment.placement == SourceTypeFragmentPlacement::UnadmittedAmbientExternalModule
            })
            .expect("ambient fragment");
        let ambient_kind = ambient.kind;
        let ambient_binding_start = ambient.binding_span.start;
        let ambient_declaration_span = ambient.declaration_span;
        let ambient_binding_span = ambient.binding_span;
        let mut builder = ProjectBinderBuilder::new_with_declaration_manifest(
            &prelude.program,
            &prelude_manifest,
        );
        let unit = CompilationUnit::implementation(SourceUnitKey::SINGLE_SOURCE, &parsed.program);
        let (module, _) =
            builder.add_module_with_declaration_manifest(&parsed.program, &manifest, &[], unit);
        let binder = builder.finish(module);
        let attached_start =
            u32::try_from(source.find("Attached").expect("Attached binding offset"))
                .expect("source offset fits u32");
        let mut attached = binder
            .exact_declaration_at(module, attached_start, DeclarationKind::TypeAlias)
            .expect("attached alias")
            .clone();
        attached.kind = ambient_kind;
        attached.site.declaration_span = ambient_declaration_span;
        attached.site.binding_span = ambient_binding_span;

        let error = BoundTypeDeclarationInventory::build_with_exact_lookup(
            &binder,
            module,
            SourceUnitKey::SINGLE_SOURCE,
            manifest,
            None,
            |lookup_module, binding_start, kind| {
                if lookup_module == module
                    && binding_start == ambient_binding_start
                    && kind == ambient_kind
                {
                    Some(&attached)
                } else {
                    binder.exact_declaration_at(lookup_module, binding_start, kind)
                }
            },
        )
        .expect_err("semantic admission of an ambient source-only row is a hard error");
        assert_eq!(
            error,
            InventoryBuildError::UnexpectedSemanticAdmission {
                declaration: attached.id,
                placement: SourceTypeFragmentPlacement::UnadmittedAmbientExternalModule,
            }
        );
    }
}

//! Historical construction observations, not a lookup basis or resolved reference census.

use super::namespace::{NamespaceTable, SourceUnitKey};
use super::scope::{ScopeGraph, ScopeId, ScopeKind};
use super::source_references::{
    ReferenceCoverage, SourceReferenceManifest, SourceRegionId, SourceScopeAnchor,
};
use crate::source::CompilationOrigin;
use crate::span::Span;
use crate::types::layered::LayeredMap;
use oxc_ast::AstType;
use rustc_hash::FxHashMap;
use std::collections::hash_map::Entry;
use std::sync::Arc;

#[cfg(test)]
thread_local! {
    static COLLECTION_ERROR_ON_FINISH: std::cell::Cell<Option<ScopeId>> = const { std::cell::Cell::new(None) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InputError {
    UnknownModule,
    WrongSource,
    ForeignInput,
    DuplicateInstall,
    Unavailable,
    InvalidProjection,
    DuplicateAnchor,
    MissingAnchor,
    ConflictingAttachment,
    MissingRoot,
    InvalidScope,
    InvalidNormalization,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ScopeAttachment {
    pub anchor: SourceScopeAnchor,
    pub region: SourceRegionId,
    pub scope: ScopeId,
    pub kind: ScopeKind,
    pub creation_parent: Option<ScopeId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RegionObservation {
    Attached(ScopeAttachment),
    MissingAttachment,
    NoRuntimeScopeHook,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RootNormalization {
    pub module: ScopeId,
    pub source: SourceUnitKey,
    pub old_parent: Option<ScopeId>,
    pub new_parent: Option<ScopeId>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct InputWork {
    pub indexed_operations: usize,
    pub indexed_regions: usize,
    pub hook_probes: usize,
    pub finalized_regions: usize,
    pub source_rows: usize,
}

#[derive(Debug)]
pub(super) struct ConsumedInput {
    pub module: ScopeId,
    pub source: SourceUnitKey,
    pub facts: Arc<SourceReferenceManifest>,
    pub origin: Option<CompilationOrigin>,
    pub regions: Vec<RegionObservation>,
    pub module_creation_parent: Option<ScopeId>,
    pub normalization: Option<RootNormalization>,
    pub errors: Vec<InputError>,
    anchors: FxHashMap<(AstType, u32, u32), SourceRegionId>,
}

impl ConsumedInput {
    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn anchor_regions(&self) -> impl Iterator<Item = SourceRegionId> + '_ {
        self.anchors.values().copied()
    }
}

#[derive(Default)]
pub(super) struct SourceInputs {
    inputs: FxHashMap<ScopeId, ConsumedInput>,
    pub errors: Vec<(ScopeId, InputError)>,
    pub work: InputWork,
}

fn has_scope_hook(syntax: AstType) -> bool {
    matches!(
        syntax,
        AstType::Program
            | AstType::Function
            | AstType::ArrowFunctionExpression
            | AstType::BlockStatement
            | AstType::ForStatement
            | AstType::ForInStatement
            | AstType::ForOfStatement
            | AstType::SwitchStatement
            | AstType::CatchClause
    )
}

impl SourceInputs {
    #[cfg(test)]
    pub(super) fn inject_collection_error_on_next_finish_for_test(module: ScopeId) {
        COLLECTION_ERROR_ON_FINISH.set(Some(module));
    }

    pub(super) fn install(
        &mut self,
        module: ScopeId,
        source: SourceUnitKey,
        facts: Arc<SourceReferenceManifest>,
        module_creation_parent: Option<ScopeId>,
        origin: Option<CompilationOrigin>,
    ) {
        let Entry::Vacant(entry) = self.inputs.entry(module) else {
            if let Some(input) = self.inputs.get_mut(&module) {
                input.errors.push(InputError::DuplicateInstall);
            }
            return;
        };
        let mut input = ConsumedInput {
            module,
            source,
            facts,
            module_creation_parent,
            origin,
            normalization: None,
            regions: Vec::new(),
            errors: Vec::new(),
            anchors: FxHashMap::default(),
        };
        match input.facts.coverage() {
            ReferenceCoverage::Unavailable => input.errors.push(InputError::Unavailable),
            ReferenceCoverage::InvalidProjection => {
                input.errors.push(InputError::InvalidProjection)
            }
            ReferenceCoverage::StructuralTraversal => {}
        }
        let operations = input.facts.operations();
        for (index, operation) in operations.iter().enumerate() {
            self.work.indexed_operations += 1;
            if operation.id.index() != index
                || operation.region.index() >= input.facts.regions().len()
                || operation
                    .parent
                    .is_some_and(|parent| parent.index() >= index)
                || (operation.parent.is_none() != (index == 0))
            {
                input.errors.push(InputError::InvalidProjection);
            }
            let parent_region = operation
                .parent
                .and_then(|parent| operations.get(parent.index()))
                .map(|parent| parent.region);
            if !input
                .facts
                .regions()
                .get(operation.region.index())
                .is_some_and(|region| {
                    if region.anchor.operation == operation.id {
                        region.parent == parent_region
                    } else {
                        parent_region == Some(operation.region)
                            && region.anchor.operation.index() < index
                    }
                })
            {
                input.errors.push(InputError::InvalidProjection);
            }
        }
        for (index, region) in input.facts.regions().iter().enumerate() {
            self.work.indexed_regions += 1;
            if region.id.index() != index
                || !region.exited
                || region.parent.is_some_and(|parent| parent.index() >= index)
                || (region.parent.is_none() != (index == 0))
                || !operations
                    .get(region.anchor.operation.index())
                    .is_some_and(|operation| {
                        operation.region == region.id
                            && operation.syntax == region.anchor.syntax
                            && operation.span == region.anchor.span
                    })
            {
                input.errors.push(InputError::InvalidProjection);
            }
            if input
                .anchors
                .insert(
                    (
                        region.anchor.syntax,
                        region.anchor.span.start,
                        region.anchor.span.end,
                    ),
                    region.id,
                )
                .is_some()
            {
                input.errors.push(InputError::DuplicateAnchor);
            }
            input.regions.push(if has_scope_hook(region.anchor.syntax) {
                RegionObservation::MissingAttachment
            } else {
                RegionObservation::NoRuntimeScopeHook
            });
        }
        entry.insert(input);
    }

    pub(super) fn attach(
        &mut self,
        module: ScopeId,
        syntax: AstType,
        span: Span,
        scope: ScopeId,
        graph: &ScopeGraph,
    ) {
        self.work.hook_probes += 1;
        let Some(input) = self.inputs.get_mut(&module) else {
            self.errors.push((module, InputError::UnknownModule));
            return;
        };
        let Some(region) = input.anchors.get(&(syntax, span.start, span.end)).copied() else {
            input.errors.push(InputError::MissingAnchor);
            return;
        };
        let Some(row) = graph.get(scope) else {
            input.errors.push(InputError::InvalidScope);
            return;
        };
        let kind_matches = match syntax {
            AstType::Program => {
                matches!(row.kind, ScopeKind::Module | ScopeKind::CompilationGlobal)
            }
            AstType::Function | AstType::ArrowFunctionExpression => row.kind == ScopeKind::Function,
            _ => has_scope_hook(syntax) && row.kind == ScopeKind::Block,
        };
        if !kind_matches {
            input.errors.push(InputError::InvalidScope);
            return;
        }
        let Some(anchor_region) = input.facts.regions().get(region.index()) else {
            input.errors.push(InputError::InvalidProjection);
            return;
        };
        let Some(observation) = input.regions.get_mut(region.index()) else {
            input.errors.push(InputError::InvalidProjection);
            return;
        };
        let attachment = ScopeAttachment {
            anchor: anchor_region.anchor,
            region,
            scope,
            kind: row.kind,
            creation_parent: row.parent,
        };
        match *observation {
            RegionObservation::MissingAttachment => {
                *observation = RegionObservation::Attached(attachment);
            }
            RegionObservation::Attached(previous) if previous == attachment => {}
            _ => input.errors.push(InputError::ConflictingAttachment),
        }
    }

    pub(super) fn normalize_root(
        &mut self,
        module: ScopeId,
        source: SourceUnitKey,
        old_parent: Option<ScopeId>,
        new_parent: Option<ScopeId>,
    ) {
        let Some(input) = self.inputs.get_mut(&module) else {
            // Product library modules have no consumed syntax input.
            return;
        };
        if input.source != source
            || input.module_creation_parent != old_parent
            || input.normalization.is_some()
        {
            input.errors.push(InputError::InvalidNormalization);
            return;
        }
        input.normalization = Some(RootNormalization {
            module,
            source,
            old_parent,
            new_parent,
        });
    }

    pub(super) fn finish(
        &mut self,
        graph: &ScopeGraph,
        modules: &LayeredMap<ScopeId, SourceUnitKey>,
        namespaces: &NamespaceTable,
        prelude_module: ScopeId,
    ) {
        #[cfg(test)]
        if let Some(module) = COLLECTION_ERROR_ON_FINISH.take() {
            self.errors.push((module, InputError::UnknownModule));
        }
        for input in self.inputs.values_mut() {
            self.work.source_rows += 1;
            let origin_matches = if input.module == prelude_module {
                input.source == SourceUnitKey::PRELUDE
                    && input.origin.is_none()
                    && input.module_creation_parent.is_none()
                    && input.normalization.is_none()
            } else {
                namespaces
                    .source_unit_for_module(input.module)
                    .is_some_and(|unit| {
                        unit.source == input.source
                            && Some(unit.origin) == input.origin
                            && namespaces.compilation_origin_for_source(input.source)
                                == input.origin
                    })
            };
            if modules.get(&input.module) != Some(&input.source) || !origin_matches {
                input.errors.push(InputError::WrongSource);
            }
            let module_parent = input
                .normalization
                .map_or(input.module_creation_parent, |transition| {
                    transition.new_parent
                });
            if !graph.get(input.module).is_some_and(|scope| {
                scope.kind == ScopeKind::Module && scope.parent == module_parent
            }) {
                input.errors.push(InputError::InvalidScope);
            }
            if !matches!(input.regions.first(), Some(RegionObservation::Attached(root))
                if root.anchor.syntax == AstType::Program)
            {
                input.errors.push(InputError::MissingRoot);
            }
            for observation in &input.regions {
                self.work.finalized_regions += 1;
                let RegionObservation::Attached(attachment) = observation else {
                    continue;
                };
                let expected_parent = if attachment.scope == input.module {
                    input
                        .normalization
                        .map_or(attachment.creation_parent, |transition| {
                            transition.new_parent
                        })
                } else {
                    attachment.creation_parent
                };
                if !graph.get(attachment.scope).is_some_and(|scope| {
                    scope.kind == attachment.kind && scope.parent == expected_parent
                }) {
                    input.errors.push(InputError::InvalidScope);
                }
            }
        }
    }

    // C1b will consume these observations only after validating its exact dependency basis.
    #[allow(dead_code)]
    pub(super) fn get(&self, module: ScopeId) -> Option<&ConsumedInput> {
        self.inputs.get(&module)
    }

    #[allow(dead_code)]
    pub(super) fn input(
        &self,
        module: ScopeId,
        source: SourceUnitKey,
        facts: &Arc<SourceReferenceManifest>,
    ) -> Result<&ConsumedInput, InputError> {
        let input = self.get(module).ok_or(InputError::UnknownModule)?;
        if input.source != source {
            return Err(InputError::WrongSource);
        }
        if !Arc::ptr_eq(&input.facts, facts) {
            return Err(InputError::ForeignInput);
        }
        if let Some(error) = input.errors.first() {
            return Err(*error);
        }
        Ok(input)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn iter(&self) -> impl Iterator<Item = &ConsumedInput> {
        self.inputs.values()
    }
}

//! Compilation-global root rows: the ordered name index every global lookup keys on.

use super::bind::Binder;
use super::declaration::{
    SourceBindingSlot, SourceGlobalBindingCandidate, SourceGlobalContributorKind, TypeGroupId,
    ValueStorageId,
};
use super::namespace::{
    DeclarationOwner, DeclarationSyntaxFacts, MergeDeclarationKind, MergeDisposition, NamespaceId,
    NamespaceInstanceState, NamespaceTable, VariableKind,
};
#[cfg(any(test, feature = "test-utils", feature = "session-bench"))]
use super::scope::ScopeKind;
#[cfg(any(test, feature = "test-utils", feature = "session-bench"))]
use crate::source::CompilationOrigin;
use crate::source::LibraryFileOrdinal;
use crate::span::Span;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Stable semantic target of one sparse frozen-prefix binder replacement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrozenLibraryMutationOwner {
    TypeGroup(TypeGroupId),
    Value(ValueStorageId),
    Namespace(NamespaceId),
}

impl PartialOrd for FrozenLibraryMutationOwner {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FrozenLibraryMutationOwner {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let key = |owner: &Self| match owner {
            Self::TypeGroup(id) => (0_u8, id.0),
            Self::Value(id) => (1, id.0),
            Self::Namespace(id) => (2, id.0),
        };
        key(self).cmp(&key(other))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootIndexError {
    MissingCompilationGlobalScope,
    MissingGlobalSymbol,
    #[cfg(any(test, feature = "test-utils", feature = "session-bench"))]
    MissingCompilationOrigin(super::namespace::SourceUnitKey),
}

impl fmt::Display for RootIndexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCompilationGlobalScope => {
                formatter.write_str("missing compilation-global scope")
            }
            Self::MissingGlobalSymbol => formatter.write_str("global symbol id is missing"),
            #[cfg(any(test, feature = "test-utils", feature = "session-bench"))]
            Self::MissingCompilationOrigin(source) => {
                write!(formatter, "source {:?} has no compilation origin", source)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootNameRow {
    pub name: String,
    pub value: Option<ValueStorageId>,
    pub ty: Option<TypeGroupId>,
    pub namespace: Option<NamespaceId>,
}

/// One admitted declaration that contributes a property to the final global object.
#[cfg(any(test, feature = "test-utils", feature = "session-bench"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FinalGlobalContributorSite {
    pub declaration: super::declaration::DeclId,
    pub source: super::namespace::SourceUnitKey,
    pub origin: CompilationOrigin,
    pub span: Span,
    pub kind: SourceGlobalContributorKind,
}

/// One unfiltered compilation-global contributor with its final published slots.
#[cfg(any(test, feature = "test-utils", feature = "session-bench"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FinalGlobalContributorRow {
    pub name: String,
    pub value: Option<ValueStorageId>,
    pub ty: Option<TypeGroupId>,
    pub namespace: Option<NamespaceId>,
    pub sites: Vec<FinalGlobalContributorSite>,
}

/// Final global-object contributor inventory across library and user sources.
#[cfg(any(test, feature = "test-utils", feature = "session-bench"))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FinalGlobalContributorInventory {
    pub rows: Vec<FinalGlobalContributorRow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryRootContributorSite {
    pub name: String,
    pub kind: SourceGlobalContributorKind,
    pub file_ordinal: LibraryFileOrdinal,
    pub span: Span,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LibraryRootProjection {
    pub candidates: BTreeMap<String, SourceGlobalBindingCandidate>,
    pub explicit_global_this: bool,
    pub contributor_sites: Vec<LibraryRootContributorSite>,
    pub explicit_global_this_sites: Vec<(LibraryFileOrdinal, Span)>,
    pub root_rows: Vec<RootNameRow>,
    pub canonical_unit_count: usize,
    pub source_census_count: usize,
    pub uncertain_candidate_count: usize,
    pub uncertain_relevant_syntax_count: usize,
    pub normalization_issue_count: usize,
}

impl LibraryRootProjection {
    pub(crate) fn observe_finalized_global_merge(
        &mut self,
        record: &super::namespace::MergeRecord,
        namespaces: &NamespaceTable,
        namespace_contributors: &mut BTreeSet<String>,
    ) {
        if record.owner != DeclarationOwner::CompilationGlobal {
            return;
        }
        if record.classification.disposition != MergeDisposition::Admitted {
            self.candidates.remove(record.name.as_ref());
            return;
        }
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
            ordinary_contributor |= declaration.kind == MergeDeclarationKind::Function
                || matches!(
                    declaration.syntax,
                    DeclarationSyntaxFacts::Variable(VariableKind::Var)
                );
        }
        let namespace = record
            .declarations
            .iter()
            .find_map(|declaration| declaration.namespace_fragment)
            .and_then(|fragment| namespaces.fragment(fragment))
            .map(|fragment| fragment.namespace);
        let namespace_instantiated = namespace.is_some_and(|namespace| {
            namespaces.aggregate_instance_state(namespace)
                == Some(NamespaceInstanceState::Instantiated)
        });
        if namespace_instantiated {
            slots.insert(SourceBindingSlot::Value);
            namespace_contributors.insert(record.name.to_string());
        }
        let Some(candidate) = self.candidates.get_mut(record.name.as_ref()) else {
            self.normalization_issue_count = self.normalization_issue_count.saturating_add(1);
            return;
        };
        candidate.slots = slots;
        candidate.global_object_contributor = ordinary_contributor || namespace_instantiated;
    }

    pub(crate) fn finish_namespace_normalization(
        &mut self,
        namespace_contributors: &BTreeSet<String>,
    ) {
        self.contributor_sites.retain(|site| match site.kind {
            SourceGlobalContributorKind::Ordinary => self.candidates.contains_key(&site.name),
            SourceGlobalContributorKind::Namespace => namespace_contributors.contains(&site.name),
        });
    }
}

/// Every compilation-global name, sorted, with the slots it publishes.
pub fn collect_root_rows(binder: &Binder) -> Result<Vec<RootNameRow>, RootIndexError> {
    let scope = binder
        .graph
        .get(binder.compilation_global)
        .ok_or(RootIndexError::MissingCompilationGlobalScope)?;
    let mut rows = scope
        .symbols
        .iter()
        .map(|(name, symbol)| {
            let record = binder
                .symbols
                .get(*symbol)
                .ok_or(RootIndexError::MissingGlobalSymbol)?;
            Ok(RootNameRow {
                name: name.clone(),
                value: record.value,
                ty: record.ty,
                namespace: record.ns,
            })
        })
        .collect::<Result<Vec<_>, RootIndexError>>()?;
    rows.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(rows)
}

/// Collects admitted global-object contributors from the final binder merge state.
#[cfg(any(test, feature = "test-utils", feature = "session-bench"))]
pub fn collect_final_global_object_contributors(
    binder: &Binder,
) -> Result<FinalGlobalContributorInventory, RootIndexError> {
    let mut global_scopes = Vec::new();
    let mut cursor = Some(binder.script_namespace_root);
    while let Some(scope_id) = cursor {
        let scope = binder
            .graph
            .get(scope_id)
            .ok_or(RootIndexError::MissingCompilationGlobalScope)?;
        if scope.kind == ScopeKind::CompilationGlobal {
            global_scopes.push(scope_id);
        }
        cursor = scope.parent;
    }
    let mut contributor_sites = BTreeMap::<String, Vec<FinalGlobalContributorSite>>::new();

    for record in binder.namespaces.merges() {
        let compilation_global_owner = record.owner == DeclarationOwner::CompilationGlobal
            || matches!(record.owner, DeclarationOwner::Lexical(scope) if global_scopes.contains(&scope));
        if !compilation_global_owner
            || record.classification.disposition != MergeDisposition::Admitted
        {
            continue;
        }
        let namespace_instantiated = record
            .declarations
            .iter()
            .filter_map(|declaration| declaration.namespace_fragment)
            .filter_map(|fragment| binder.namespaces.fragment(fragment))
            .any(|fragment| {
                binder
                    .namespaces
                    .aggregate_instance_state(fragment.namespace)
                    == Some(NamespaceInstanceState::Instantiated)
            });
        let mut sites = Vec::new();
        for declaration in record.declarations.iter() {
            let ordinary = declaration.kind == MergeDeclarationKind::Function
                || matches!(
                    declaration.syntax,
                    DeclarationSyntaxFacts::Variable(VariableKind::Var)
                );
            let namespace = declaration.namespace_fragment.is_some() && namespace_instantiated;
            for kind in [
                ordinary.then_some(SourceGlobalContributorKind::Ordinary),
                namespace.then_some(SourceGlobalContributorKind::Namespace),
            ]
            .into_iter()
            .flatten()
            {
                let origin = binder
                    .namespaces
                    .compilation_origin_for_source(declaration.source)
                    .ok_or(RootIndexError::MissingCompilationOrigin(declaration.source))?;
                sites.push(FinalGlobalContributorSite {
                    declaration: declaration.declaration,
                    source: declaration.source,
                    origin,
                    span: declaration.binding_span,
                    kind,
                });
            }
        }
        if sites.is_empty() {
            continue;
        }
        contributor_sites
            .entry(record.name.to_string())
            .or_default()
            .extend(sites);
    }

    let mut rows = Vec::with_capacity(contributor_sites.len());
    for (name, mut sites) in contributor_sites {
        sites.sort_by_key(|site| {
            (
                site.origin,
                site.source,
                site.span.start,
                site.span.end,
                site.kind,
                site.declaration.0,
            )
        });
        let symbol = binder
            .graph
            .resolve(binder.script_namespace_root, &name)
            .ok_or(RootIndexError::MissingGlobalSymbol)?;
        let root = binder
            .symbols
            .get(symbol)
            .ok_or(RootIndexError::MissingGlobalSymbol)?;
        rows.push(FinalGlobalContributorRow {
            name,
            value: root.value,
            ty: root.ty,
            namespace: root.ns,
            sites,
        });
    }
    Ok(FinalGlobalContributorInventory { rows })
}

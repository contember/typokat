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
}

impl fmt::Display for RootIndexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCompilationGlobalScope => {
                formatter.write_str("missing compilation-global scope")
            }
            Self::MissingGlobalSymbol => formatter.write_str("global symbol id is missing"),
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

//! Compilation-global root rows: the ordered name index every global lookup keys on.

use super::bind::Binder;
use super::declaration::{TypeGroupId, ValueStorageId};
use super::namespace::NamespaceId;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RootIndexError {
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

#[derive(Clone)]
pub(crate) struct RootNameRow {
    pub(crate) name: String,
    pub(crate) value: Option<ValueStorageId>,
    pub(crate) ty: Option<TypeGroupId>,
    pub(crate) namespace: Option<NamespaceId>,
}

/// Every compilation-global name, sorted, with the slots it publishes.
pub(crate) fn collect_root_rows(binder: &Binder) -> Result<Vec<RootNameRow>, RootIndexError> {
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

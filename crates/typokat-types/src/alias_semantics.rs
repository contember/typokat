//! Immutable recursive-alias publication domain (ADR-0024).

use crate::types::layered::LayeredMap;
use crate::types::repr::{AliasId, TypeParamId};
use crate::types::store::{Store, TypeId};
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// A complete recursive-alias body. Draft bodies never enter this registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublishedAliasSurface {
    alias: AliasId,
    template: TypeId,
    type_params: Box<[TypeParamId]>,
    body: TypeId,
}

impl PublishedAliasSurface {
    pub fn new(
        alias: AliasId,
        template: TypeId,
        type_params: Vec<TypeParamId>,
        body: TypeId,
    ) -> Self {
        Self {
            alias,
            template,
            type_params: type_params.into_boxed_slice(),
            body,
        }
    }

    pub fn alias(&self) -> AliasId {
        self.alias
    }

    pub fn template(&self) -> TypeId {
        self.template
    }

    pub fn type_params(&self) -> &[TypeParamId] {
        &self.type_params
    }

    pub fn body(&self) -> TypeId {
        self.body
    }
}

/// A final failure for one alias SCC member.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PublishedAliasUnavailable {
    DirectCycle,
    UnsupportedBody,
    ComponentFailure,
}

/// One immutable terminal. A published registry contains no resolving rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublishedAliasTerminal {
    Ready(PublishedAliasSurface),
    Unavailable(PublishedAliasUnavailable),
}

/// Immutable alias publication snapshot used by semantic queries.
pub struct PublishedAliases {
    terminals: LayeredMap<AliasId, PublishedAliasTerminal>,
    identity: Arc<()>,
}

impl Clone for PublishedAliases {
    fn clone(&self) -> Self {
        Self {
            terminals: self.terminals.clone(),
            identity: Arc::clone(&self.identity),
        }
    }
}

impl PublishedAliases {
    pub fn validate_ready_surface(
        store: &Store,
        alias: AliasId,
        surface: &PublishedAliasSurface,
    ) -> Result<(), &'static str> {
        let type_in_range = |ty: TypeId| ty.index() < store.len();
        if surface.alias() != alias {
            return Err("published alias surface has the wrong alias id");
        }
        if !type_in_range(surface.template())
            || store.alias_template_id(surface.template()) != Some(alias)
        {
            return Err("published alias surface has an invalid template");
        }
        if !type_in_range(surface.body()) {
            return Err("published alias surface has an out-of-range body");
        }
        let mut parameters = rustc_hash::FxHashSet::default();
        if surface.type_params().iter().any(|parameter| {
            store.type_param_name(*parameter).is_none() || !parameters.insert(*parameter)
        }) {
            return Err("published alias surface has invalid parameters");
        }
        Ok(())
    }

    pub fn owned_terminals(&self) -> Vec<(AliasId, PublishedAliasTerminal)> {
        let mut terminals = self
            .terminals
            .iter()
            .map(|(&alias, terminal)| (alias, terminal.clone()))
            .collect::<Vec<_>>();
        terminals.sort_by_key(|(alias, _)| *alias);
        terminals
    }

    pub fn from_owned_terminals(
        terminals: Vec<(AliasId, PublishedAliasTerminal)>,
    ) -> Result<Self, &'static str> {
        if terminals.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
            return Err("owned alias terminals are not strictly ordered");
        }
        Self::from_terminals(terminals.into_iter().collect())
            .ok_or("owned alias publication is invalid")
    }

    pub fn empty() -> Self {
        Self {
            terminals: LayeredMap::default(),
            identity: Arc::new(()),
        }
    }

    pub fn from_terminals(terminals: FxHashMap<AliasId, PublishedAliasTerminal>) -> Option<Self> {
        let valid = terminals.iter().all(|(alias, terminal)| match terminal {
            PublishedAliasTerminal::Ready(surface) => surface.alias() == *alias,
            PublishedAliasTerminal::Unavailable(_) => true,
        });
        valid.then_some(Self {
            terminals: terminals.into(),
            identity: Arc::new(()),
        })
    }

    pub fn terminal(&self, alias: AliasId) -> Option<&PublishedAliasTerminal> {
        self.terminals.get(&alias)
    }

    pub fn ready(&self, alias: AliasId) -> Option<&PublishedAliasSurface> {
        match self.terminal(alias) {
            Some(PublishedAliasTerminal::Ready(surface)) => Some(surface),
            Some(PublishedAliasTerminal::Unavailable(_)) | None => None,
        }
    }

    pub fn identity(&self) -> &Arc<()> {
        &self.identity
    }

    /// Validate that one snapshot is exact or disjoint from this publication.
    pub fn validate_snapshot(&self, snapshot: &Self) -> Result<(), &'static str> {
        if snapshot.owned_terminals().iter().any(|(alias, terminal)| {
            self.terminal(*alias)
                .is_some_and(|published| published != terminal)
        }) {
            return Err("alias snapshot conflicts with an existing terminal");
        }
        Ok(())
    }

    pub fn install_snapshot(&mut self, snapshot: &Self) -> Result<bool, &'static str> {
        self.validate_snapshot(snapshot)?;
        let terminals = snapshot.owned_terminals();
        let additions = terminals
            .into_iter()
            .filter(|(alias, _)| self.terminal(*alias).is_none())
            .collect::<Vec<_>>();
        if additions.is_empty() {
            return Ok(false);
        }
        for (alias, terminal) in additions {
            self.terminals
                .insert_local(alias, terminal)
                .map_err(|_| "alias snapshot cannot replace a sealed terminal")?;
        }
        self.identity = Arc::new(());
        Ok(true)
    }

    pub fn freeze_as_base(&mut self) -> Result<(), &'static str> {
        self.terminals.freeze_as_base()
    }

    pub fn fork_delta(&self) -> Result<Self, &'static str> {
        Ok(Self {
            terminals: self.terminals.fork_delta()?,
            identity: Arc::clone(&self.identity),
        })
    }

    pub fn extend(mut self, extension: Self) -> Option<Self> {
        if extension
            .terminals
            .keys()
            .any(|alias| self.terminals.contains_key(alias))
        {
            return None;
        }
        for (&alias, terminal) in extension.terminals.iter() {
            self.terminals.insert_local(alias, terminal.clone()).ok()?;
        }
        self.identity = Arc::new(());
        Some(self)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn local_row_count_for_test(&self) -> usize {
        self.terminals.local_len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_rejects_mismatched_alias_identity() {
        let surface = PublishedAliasSurface::new(AliasId(2), TypeId(3), Vec::new(), TypeId(5));
        assert!(PublishedAliases::from_terminals(FxHashMap::from_iter([(
            AliasId(1),
            PublishedAliasTerminal::Ready(surface),
        )]))
        .is_none());
    }

    #[test]
    fn frozen_alias_publication_shares_rows_and_isolates_extensions() {
        let alias = AliasId(1);
        let mut base = PublishedAliases::from_terminals(FxHashMap::from_iter([(
            alias,
            PublishedAliasTerminal::Ready(PublishedAliasSurface::new(
                alias,
                TypeId(3),
                vec![TypeParamId(5)],
                TypeId(7),
            )),
        )]))
        .expect("valid publication");
        base.freeze_as_base().expect("freeze alias publication");
        let first = base.fork_delta().expect("first alias delta");
        let second = base.fork_delta().expect("second alias delta");

        let local = AliasId(2);
        let extension = PublishedAliases::from_terminals(FxHashMap::from_iter([(
            local,
            PublishedAliasTerminal::Unavailable(PublishedAliasUnavailable::DirectCycle),
        )]))
        .expect("valid terminal extension");
        let extended = first.extend(extension).expect("disjoint extension");

        assert!(extended.terminal(local).is_some());
        assert!(second.terminal(local).is_none());
        assert!(base.terminal(local).is_none());
    }

    #[test]
    fn snapshot_install_is_exactly_idempotent_and_rejects_conflicts_atomically() {
        let alias = AliasId(3);
        let terminal = PublishedAliasTerminal::Ready(PublishedAliasSurface::new(
            alias,
            TypeId(5),
            vec![TypeParamId(7)],
            TypeId(11),
        ));
        let snapshot =
            PublishedAliases::from_terminals(FxHashMap::from_iter([(alias, terminal.clone())]))
                .expect("valid snapshot");
        let mut installed = PublishedAliases::empty();
        let initial_identity = Arc::clone(installed.identity());

        assert_eq!(installed.install_snapshot(&snapshot), Ok(true));
        assert!(!Arc::ptr_eq(&initial_identity, installed.identity()));
        let published_identity = Arc::clone(installed.identity());
        assert_eq!(installed.install_snapshot(&snapshot), Ok(false));
        assert!(Arc::ptr_eq(&published_identity, installed.identity()));

        let conflict = PublishedAliases::from_terminals(FxHashMap::from_iter([
            (
                alias,
                PublishedAliasTerminal::Unavailable(PublishedAliasUnavailable::ComponentFailure),
            ),
            (
                AliasId(13),
                PublishedAliasTerminal::Unavailable(PublishedAliasUnavailable::DirectCycle),
            ),
        ]))
        .expect("valid conflicting snapshot");
        assert!(installed.install_snapshot(&conflict).is_err());
        assert_eq!(installed.terminal(alias), Some(&terminal));
        assert!(installed.terminal(AliasId(13)).is_none());
        assert!(Arc::ptr_eq(&published_identity, installed.identity()));
    }
}

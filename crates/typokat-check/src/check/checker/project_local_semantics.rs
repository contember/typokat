//! Project-owned demand scheduling for immutable library declaration products.

#[cfg(test)]
use std::cell::Cell;
use std::collections::BTreeMap;
#[cfg(any(test, feature = "test-utils"))]
use std::collections::BTreeSet;

use crate::binder::namespace::LibraryProductBinderMap;
use crate::binder::symbol::SymbolId;
use crate::binder::Binder;
use crate::check::checker::decls::inventory::BoundTypeDeclarationInventory;
use oxc_ast::ast::TSTypeName;
use oxc_ast_visit::{walk, Visit};
#[cfg(any(test, feature = "test-utils"))]
use rustc_hash::FxHashMap;
use typokat_library_product::{DependencyTarget, LibraryDeclarationProduct};

#[cfg(test)]
thread_local! {
    static LIBRARY_TYPE_DEPENDENCY_RECONSTRUCTION_WORK: Cell<LibraryTypeDependencyReconstructionWorkForTest> = const {
        Cell::new(LibraryTypeDependencyReconstructionWorkForTest {
            invocations: 0,
            product_dependency_rows: 0,
        })
    };
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::check::checker) struct LibraryTypeDependencyReconstructionWorkForTest {
    pub invocations: usize,
    pub product_dependency_rows: usize,
}

#[cfg(test)]
impl LibraryTypeDependencyReconstructionWorkForTest {
    fn saturating_sub(self, start: Self) -> Self {
        Self {
            invocations: self.invocations.saturating_sub(start.invocations),
            product_dependency_rows: self
                .product_dependency_rows
                .saturating_sub(start.product_dependency_rows),
        }
    }
}

#[cfg(test)]
pub(in crate::check::checker) struct LibraryTypeDependencyReconstructionScopeForTest(
    LibraryTypeDependencyReconstructionWorkForTest,
);

#[cfg(test)]
impl LibraryTypeDependencyReconstructionScopeForTest {
    pub fn start() -> Self {
        Self(LIBRARY_TYPE_DEPENDENCY_RECONSTRUCTION_WORK.get())
    }

    pub fn finish(self) -> LibraryTypeDependencyReconstructionWorkForTest {
        LIBRARY_TYPE_DEPENDENCY_RECONSTRUCTION_WORK
            .get()
            .saturating_sub(self.0)
    }
}

#[cfg(test)]
fn record_library_type_dependency_reconstruction_invocation_for_test() {
    LIBRARY_TYPE_DEPENDENCY_RECONSTRUCTION_WORK.with(|work| {
        let mut current = work.get();
        current.invocations = current.invocations.saturating_add(1);
        work.set(current);
    });
}

#[cfg(test)]
fn record_library_type_dependency_reconstruction_rows_for_test(rows: usize) {
    LIBRARY_TYPE_DEPENDENCY_RECONSTRUCTION_WORK.with(|work| {
        let mut current = work.get();
        current.product_dependency_rows = current.product_dependency_rows.saturating_add(rows);
        work.set(current);
    });
}

pub(super) fn library_type_dependencies(
    product: &LibraryDeclarationProduct,
    map: &LibraryProductBinderMap,
) -> Result<Vec<(SymbolId, SymbolId)>, &'static str> {
    #[cfg(test)]
    record_library_type_dependency_reconstruction_invocation_for_test();
    let mut rows = Vec::new();
    for dependency in product.dependency_rows() {
        let DependencyTarget::Group(target) = dependency.target else {
            continue;
        };
        let owner = map
            .group(dependency.owner)
            .map(|binding| binding.symbol())
            .ok_or("library dependency owner has no local symbol")?;
        let target = map
            .group(target)
            .map(|binding| binding.symbol())
            .ok_or("library dependency target has no local symbol")?;
        rows.push((owner, target));
    }
    rows.sort_by_key(|(owner, target)| (owner.index(), target.index()));
    rows.dedup();
    #[cfg(test)]
    record_library_type_dependency_reconstruction_rows_for_test(rows.len());
    Ok(rows)
}

#[cfg(any(test, feature = "test-utils"))]
pub(super) fn project_local_semantic_owners(
    product: &LibraryDeclarationProduct,
    map: &LibraryProductBinderMap,
    binder: &Binder,
    inventories: &[BoundTypeDeclarationInventory<'_>],
) -> Result<Vec<SymbolId>, &'static str> {
    let mut owners = product
        .groups()
        .iter()
        .enumerate()
        .map(|(index, _)| {
            product
                .group_key_at(index)
                .and_then(|key| map.group(key))
                .map(|binding| binding.symbol())
                .ok_or("library semantic group has no local symbol")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (type_symbols, declaration_symbols) = product_type_identity_maps(product, map, binder);
    owners.extend(inventories.iter().flat_map(|inventory| {
        inventory.declarations().iter().filter_map(|row| {
            resolve_user_declaration_owner(binder, &type_symbols, &declaration_symbols, row)
        })
    }));
    owners.sort_by_key(|owner| owner.index());
    owners.dedup();
    Ok(owners)
}

pub(super) fn user_type_dependencies<'ast>(
    product: &LibraryDeclarationProduct,
    map: &LibraryProductBinderMap,
    binder: &Binder,
    inventories: &[BoundTypeDeclarationInventory<'ast>],
) -> Vec<(SymbolId, SymbolId)> {
    let (type_symbols, declaration_symbols) = product_type_identity_maps(product, map, binder);
    let mut dependencies = Vec::new();
    for inventory in inventories {
        for row in inventory.declarations().iter() {
            let Some(owner) =
                resolve_user_declaration_owner(binder, &type_symbols, &declaration_symbols, row)
            else {
                continue;
            };
            let mut visitor = UserTypeDependencyVisitor {
                binder,
                scope: row.scope,
                owner,
                type_symbols: &type_symbols,
                dependencies: &mut dependencies,
            };
            match row.payload {
                crate::binder::declaration::SourceTypeFragmentPayload::TypeAlias(value) => {
                    visitor.visit_ts_type_alias_declaration(value)
                }
                crate::binder::declaration::SourceTypeFragmentPayload::Interface(value) => {
                    visitor.visit_ts_interface_declaration(value)
                }
                crate::binder::declaration::SourceTypeFragmentPayload::Class(value) => {
                    visitor.visit_class(value)
                }
                crate::binder::declaration::SourceTypeFragmentPayload::Enum(value) => {
                    visitor.visit_ts_enum_declaration(value)
                }
            }
        }
    }
    dependencies.sort_by_key(|(owner, target)| (owner.index(), target.index()));
    dependencies.dedup();
    dependencies
}

fn product_type_identity_maps(
    product: &LibraryDeclarationProduct,
    map: &LibraryProductBinderMap,
    binder: &Binder,
) -> (
    BTreeMap<crate::binder::declaration::TypeGroupId, SymbolId>,
    Vec<Option<SymbolId>>,
) {
    let type_symbols = product
        .groups()
        .iter()
        .enumerate()
        .filter_map(|(index, _)| {
            product
                .group_key_at(index)
                .and_then(|key| map.group(key))
                .and_then(|binding| binding.ty().map(|ty| (ty, binding.symbol())))
        })
        .collect::<BTreeMap<_, _>>();
    let mut declaration_symbols = vec![None; binder.declarations.len()];
    for (index, group) in product.groups().iter().enumerate() {
        let symbol = product
            .group_key_at(index)
            .and_then(|key| map.group(key))
            .map(|binding| binding.symbol());
        for declaration in &group.fragments {
            if let (Some(symbol), Some(binding)) = (symbol, map.declaration(*declaration)) {
                declaration_symbols[binding.declaration().index()] = Some(symbol);
            }
        }
    }
    (type_symbols, declaration_symbols)
}

fn resolve_user_declaration_owner(
    binder: &Binder,
    type_symbols: &BTreeMap<crate::binder::declaration::TypeGroupId, SymbolId>,
    declaration_symbols: &[Option<SymbolId>],
    row: &crate::check::checker::decls::inventory::BoundTypeDeclaration<'_>,
) -> Option<SymbolId> {
    resolve_declaration_merge_symbol(binder, type_symbols, declaration_symbols, row.declaration)
        .or_else(|| type_symbols.get(&row.group).copied())
        .or_else(|| {
            binder.type_groups.get(row.group).and_then(|group| {
                resolve_final_type_symbol(binder, type_symbols, row.scope, &group.name)
            })
        })
}

struct UserTypeDependencyVisitor<'a> {
    binder: &'a Binder,
    scope: crate::binder::scope::ScopeId,
    owner: SymbolId,
    type_symbols: &'a BTreeMap<crate::binder::declaration::TypeGroupId, SymbolId>,
    dependencies: &'a mut Vec<(SymbolId, SymbolId)>,
}

impl UserTypeDependencyVisitor<'_> {
    fn canonical_symbol(&self, symbol: SymbolId) -> SymbolId {
        canonical_type_symbol(self.binder, self.type_symbols, symbol)
    }
}

fn canonical_type_symbol(
    binder: &Binder,
    type_symbols: &BTreeMap<crate::binder::declaration::TypeGroupId, SymbolId>,
    symbol: SymbolId,
) -> SymbolId {
    binder
        .symbols
        .get(symbol)
        .and_then(|row| row.ty)
        .and_then(|group| type_symbols.get(&group).copied())
        .unwrap_or(symbol)
}

fn resolve_final_type_symbol(
    binder: &Binder,
    type_symbols: &BTreeMap<crate::binder::declaration::TypeGroupId, SymbolId>,
    scope: crate::binder::scope::ScopeId,
    name: &str,
) -> Option<SymbolId> {
    let owner_public = match binder.namespaces.declaration_owner_for_scope(scope) {
        crate::binder::namespace::DeclarationOwner::NamespacePublic(namespace) => binder
            .namespaces
            .get(namespace)
            .map(|namespace| namespace.public_scope),
        crate::binder::namespace::DeclarationOwner::NamespacePrivate(fragment) => binder
            .namespaces
            .fragment(fragment)
            .and_then(|fragment| binder.namespaces.get(fragment.namespace))
            .map(|namespace| namespace.public_scope),
        crate::binder::namespace::DeclarationOwner::Lexical(_)
        | crate::binder::namespace::DeclarationOwner::CompilationGlobal
        | crate::binder::namespace::DeclarationOwner::DeferredAmbientModule(_) => None,
    };
    let public = owner_public
        .or_else(|| {
            binder
                .graph
                .get(scope)
                .and_then(|scope| scope.namespace_public)
        })
        .and_then(|public| binder.graph.get(public))
        .and_then(|public| public.lookup_local(name));
    public
        .or_else(|| binder.resolve_type(scope, name))
        .map(|symbol| canonical_type_symbol(binder, type_symbols, symbol))
}

fn resolve_declaration_merge_symbol(
    binder: &Binder,
    type_symbols: &BTreeMap<crate::binder::declaration::TypeGroupId, SymbolId>,
    declaration_symbols: &[Option<SymbolId>],
    declaration: crate::binder::declaration::DeclId,
) -> Option<SymbolId> {
    if let Some(symbol) = binder
        .namespaces
        .merged_declarations_for_declaration(declaration)
        .and_then(|participants| {
            participants.iter().find_map(|participant| {
                declaration_symbols
                    .get(participant.declaration.index())
                    .copied()
                    .flatten()
            })
        })
    {
        return Some(symbol);
    }
    let (owner, name) = binder
        .namespaces
        .merge_identity_for_declaration(declaration)?;
    let scope = match owner {
        crate::binder::namespace::DeclarationOwner::Lexical(scope) => scope,
        crate::binder::namespace::DeclarationOwner::NamespacePublic(namespace) => {
            binder.namespaces.get(namespace)?.public_scope
        }
        crate::binder::namespace::DeclarationOwner::NamespacePrivate(fragment) => {
            let namespace = binder.namespaces.fragment(fragment)?.namespace;
            binder.namespaces.get(namespace)?.public_scope
        }
        crate::binder::namespace::DeclarationOwner::CompilationGlobal => binder.compilation_global,
        crate::binder::namespace::DeclarationOwner::DeferredAmbientModule(_) => return None,
    };
    binder
        .graph
        .get(scope)
        .and_then(|scope| scope.lookup_local(name))
        .map(|symbol| canonical_type_symbol(binder, type_symbols, symbol))
}

impl<'ast> Visit<'ast> for UserTypeDependencyVisitor<'_> {
    fn visit_ts_type_name(&mut self, name: &TSTypeName<'ast>) {
        let mut segments = Vec::new();
        if !flatten_type_name(name, &mut segments) {
            return;
        }
        let target = match segments.as_slice() {
            [name] => self
                .binder
                .resolve_type(self.scope, name)
                .map(|symbol| self.canonical_symbol(symbol)),
            [_, _, ..] => match self
                .binder
                .resolve_qualified_type_path(self.scope, &segments)
            {
                crate::binder::namespace::QualifiedTypePathResolution::TypeGroup(group) => {
                    self.type_symbols.get(&group).copied().or_else(|| {
                        self.binder
                            .symbols
                            .iter()
                            .find_map(|(id, symbol)| (symbol.ty == Some(group)).then_some(id))
                    })
                }
                _ => None,
            },
            [] => None,
        };
        if let Some(target) = target {
            self.dependencies.push((self.owner, target));
        }
    }

    fn visit_class(&mut self, class: &oxc_ast::ast::Class<'ast>) {
        if let Some(oxc_ast::ast::Expression::Identifier(identifier)) = &class.super_class {
            if let Some(target) = self
                .binder
                .resolve_type(self.scope, identifier.name.as_str())
                .map(|symbol| self.canonical_symbol(symbol))
            {
                self.dependencies.push((self.owner, target));
            }
        }
        walk::walk_class(self, class);
    }
}

fn flatten_type_name<'a>(name: &'a TSTypeName<'_>, output: &mut Vec<&'a str>) -> bool {
    match name {
        TSTypeName::IdentifierReference(identifier) => {
            output.push(identifier.name.as_str());
            true
        }
        TSTypeName::QualifiedName(qualified) => {
            if !flatten_type_name(&qualified.left, output) {
                return false;
            }
            output.push(qualified.right.name.as_str());
            true
        }
        TSTypeName::ThisExpression(_) => false,
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::check) enum ProjectLocalTerminal<Ready, Unavailable> {
    Unresolved,
    Resolving { component: usize },
    Ready(Ready),
    Unavailable(Unavailable),
}

#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::check) enum ProjectLocalObservation<'a, Ready, Unavailable> {
    Unresolved,
    Resolving,
    Ready(&'a Ready),
    Unavailable(&'a Unavailable),
}

#[cfg(any(test, feature = "test-utils"))]
pub(in crate::check) trait ProjectLocalSemanticPublisher {
    type Ready;
    type Unavailable: Clone;
    type Error;

    fn begin_component(&mut self, members: &[SymbolId]);
    fn lower_component(
        &mut self,
        members: &[SymbolId],
    ) -> ProjectLocalComponentResults<Self::Ready, Self::Unavailable, Self::Error>;
    fn commit_component(&mut self, members: &[SymbolId]);
    fn rollback_component(&mut self, members: &[SymbolId]);
}

#[cfg(any(test, feature = "test-utils"))]
type ProjectLocalComponentResults<Ready, Unavailable, Error> =
    Result<Vec<Result<Ready, Unavailable>>, Error>;

#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, PartialEq, Eq)]
pub(in crate::check) enum ProjectLocalDemandError<Error> {
    UnknownGroup(SymbolId),
    PublisherCardinality { expected: usize, actual: usize },
    Publisher(Error),
}

#[cfg(any(test, feature = "test-utils"))]
pub(in crate::check) struct ProjectLocalSemanticCoordinator<Ready, Unavailable> {
    components: Vec<Box<[SymbolId]>>,
    owner_indices: FxHashMap<SymbolId, usize>,
    component_by_owner: Vec<usize>,
    dependencies: Vec<Box<[usize]>>,
    terminals: Vec<ProjectLocalTerminal<Ready, Unavailable>>,
}

#[cfg(any(test, feature = "test-utils"))]
impl<Ready, Unavailable> ProjectLocalSemanticCoordinator<Ready, Unavailable> {
    pub(in crate::check) fn new(
        owners: impl IntoIterator<Item = SymbolId>,
        dependencies: impl IntoIterator<Item = (SymbolId, SymbolId)>,
    ) -> Self {
        let mut owners = owners.into_iter().collect::<Vec<_>>();
        owners.sort_by_key(|owner| owner.index());
        owners.dedup();
        let owner_indices = owners
            .iter()
            .copied()
            .enumerate()
            .map(|(index, owner)| (owner, index))
            .collect::<FxHashMap<_, _>>();
        let mut edges = vec![Vec::new(); owners.len()];
        for (owner, dependency) in dependencies {
            let owner = *owner_indices
                .get(&owner)
                .expect("dependency owner must be admitted");
            let dependency = *owner_indices
                .get(&dependency)
                .expect("dependency target must be admitted");
            edges[owner].push(dependency);
        }
        for targets in &mut edges {
            targets.sort_unstable();
            targets.dedup();
        }
        let components = dependency_sccs(&edges)
            .into_iter()
            .map(|component| {
                component
                    .into_iter()
                    .map(|index| owners[index])
                    .collect::<Vec<_>>()
                    .into_boxed_slice()
            })
            .collect::<Vec<_>>();
        let mut component_by_owner = vec![usize::MAX; owners.len()];
        for (component, members) in components.iter().enumerate() {
            for member in members {
                component_by_owner[owner_indices[member]] = component;
            }
        }
        let mut component_dependencies = vec![BTreeSet::new(); components.len()];
        for (owner, targets) in edges.iter().enumerate() {
            for target in targets {
                let owner_component = component_by_owner[owner];
                let target_component = component_by_owner[*target];
                if owner_component != target_component {
                    component_dependencies[owner_component].insert(target_component);
                }
            }
        }
        Self {
            components,
            owner_indices,
            component_by_owner,
            dependencies: component_dependencies
                .into_iter()
                .map(|rows| rows.into_iter().collect())
                .collect(),
            terminals: std::iter::repeat_with(|| ProjectLocalTerminal::Unresolved)
                .take(owners.len())
                .collect(),
        }
    }

    pub(in crate::check) fn observe(
        &self,
        group: SymbolId,
    ) -> ProjectLocalObservation<'_, Ready, Unavailable> {
        let Some(index) = self.owner_indices.get(&group).copied() else {
            return ProjectLocalObservation::Unresolved;
        };
        match self.terminals.get(index) {
            Some(ProjectLocalTerminal::Resolving { .. }) => ProjectLocalObservation::Resolving,
            Some(ProjectLocalTerminal::Ready(value)) => ProjectLocalObservation::Ready(value),
            Some(ProjectLocalTerminal::Unavailable(value)) => {
                ProjectLocalObservation::Unavailable(value)
            }
            Some(ProjectLocalTerminal::Unresolved) | None => ProjectLocalObservation::Unresolved,
        }
    }

    pub(in crate::check) fn demand<Publisher>(
        &mut self,
        group: SymbolId,
        publisher: &mut Publisher,
    ) -> Result<
        ProjectLocalObservation<'_, Ready, Unavailable>,
        ProjectLocalDemandError<Publisher::Error>,
    >
    where
        Publisher: ProjectLocalSemanticPublisher<Ready = Ready, Unavailable = Unavailable>,
        Unavailable: Clone,
    {
        let Some(owner) = self.owner_indices.get(&group).copied() else {
            return Err(ProjectLocalDemandError::UnknownGroup(group));
        };
        let component = self.component_by_owner[owner];
        self.publish_component(component, publisher)?;
        Ok(self.observe(group))
    }

    fn publish_component<Publisher>(
        &mut self,
        component: usize,
        publisher: &mut Publisher,
    ) -> Result<(), ProjectLocalDemandError<Publisher::Error>>
    where
        Publisher: ProjectLocalSemanticPublisher<Ready = Ready, Unavailable = Unavailable>,
        Unavailable: Clone,
    {
        let members = self.components[component].clone();
        if members.iter().all(|member| {
            let index = self.owner_indices[member];
            matches!(
                self.terminals[index],
                ProjectLocalTerminal::Ready(_) | ProjectLocalTerminal::Unavailable(_)
            )
        }) {
            return Ok(());
        }
        for dependency in self.dependencies[component].clone() {
            self.publish_component(dependency, publisher)?;
        }
        for member in &members {
            let index = self.owner_indices[member];
            self.terminals[index] = ProjectLocalTerminal::Resolving { component };
        }
        publisher.begin_component(&members);
        let results = match publisher.lower_component(&members) {
            Ok(results) => results,
            Err(error) => {
                publisher.rollback_component(&members);
                for member in &members {
                    let index = self.owner_indices[member];
                    self.terminals[index] = ProjectLocalTerminal::Unresolved;
                }
                return Err(ProjectLocalDemandError::Publisher(error));
            }
        };
        if results.len() != members.len() {
            publisher.rollback_component(&members);
            for member in &members {
                let index = self.owner_indices[member];
                self.terminals[index] = ProjectLocalTerminal::Unresolved;
            }
            return Err(ProjectLocalDemandError::PublisherCardinality {
                expected: members.len(),
                actual: results.len(),
            });
        }
        let unavailable = results
            .iter()
            .find_map(|result| result.as_ref().err().cloned());
        if let Some(unavailable) = unavailable {
            publisher.rollback_component(&members);
            for member in &members {
                let index = self.owner_indices[member];
                self.terminals[index] = ProjectLocalTerminal::Unavailable(unavailable.clone());
            }
        } else {
            publisher.commit_component(&members);
            for (member, result) in members.iter().zip(results) {
                let Ok(ready) = result else {
                    unreachable!("unavailable component handled before commit");
                };
                let index = self.owner_indices[member];
                self.terminals[index] = ProjectLocalTerminal::Ready(ready);
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub(in crate::check) fn force_all<Publisher>(
        &mut self,
        publisher: &mut Publisher,
    ) -> Result<(), ProjectLocalDemandError<Publisher::Error>>
    where
        Publisher: ProjectLocalSemanticPublisher<Ready = Ready, Unavailable = Unavailable>,
        Unavailable: Clone,
    {
        for component in 0..self.components.len() {
            self.publish_component(component, publisher)?;
        }
        Ok(())
    }

    #[cfg(test)]
    pub(in crate::check) fn components_for_test(&self) -> Vec<Vec<SymbolId>> {
        self.components
            .iter()
            .map(|members| members.to_vec())
            .collect()
    }
}

#[cfg(any(test, feature = "test-utils"))]
fn dependency_sccs(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    struct Tarjan<'a> {
        edges: &'a [Vec<usize>],
        index: usize,
        indices: Vec<Option<usize>>,
        lowlink: Vec<usize>,
        stack: Vec<usize>,
        on_stack: Vec<bool>,
        components: Vec<Vec<usize>>,
    }

    impl Tarjan<'_> {
        fn visit(&mut self, node: usize) {
            self.indices[node] = Some(self.index);
            self.lowlink[node] = self.index;
            self.index += 1;
            self.stack.push(node);
            self.on_stack[node] = true;
            for target in &self.edges[node] {
                if self.indices[*target].is_none() {
                    self.visit(*target);
                    self.lowlink[node] = self.lowlink[node].min(self.lowlink[*target]);
                } else if self.on_stack[*target] {
                    self.lowlink[node] =
                        self.lowlink[node].min(self.indices[*target].expect("visited"));
                }
            }
            if self.lowlink[node] == self.indices[node].expect("visited") {
                let mut component = Vec::new();
                loop {
                    let member = self.stack.pop().expect("SCC stack");
                    self.on_stack[member] = false;
                    component.push(member);
                    if member == node {
                        break;
                    }
                }
                component.sort_unstable();
                self.components.push(component);
            }
        }
    }

    let mut tarjan = Tarjan {
        edges,
        index: 0,
        indices: vec![None; edges.len()],
        lowlink: vec![0; edges.len()],
        stack: Vec::new(),
        on_stack: vec![false; edges.len()],
        components: Vec::new(),
    };
    for node in 0..edges.len() {
        if tarjan.indices[node].is_none() {
            tarjan.visit(node);
        }
    }
    tarjan.components.sort();
    tarjan.components
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Publisher {
        next: usize,
        begins: Vec<Vec<SymbolId>>,
        commits: Vec<Vec<SymbolId>>,
        rollbacks: Vec<Vec<SymbolId>>,
        unavailable: Option<SymbolId>,
        fail: bool,
        short: bool,
    }

    impl ProjectLocalSemanticPublisher for Publisher {
        type Ready = usize;
        type Unavailable = &'static str;
        type Error = &'static str;
        fn begin_component(&mut self, members: &[SymbolId]) {
            self.begins.push(members.to_vec());
        }
        fn lower_component(
            &mut self,
            members: &[SymbolId],
        ) -> ProjectLocalComponentResults<usize, &'static str, &'static str> {
            if self.fail {
                return Err("publisher failure");
            }
            let mut results = members
                .iter()
                .map(|member| {
                    if self.unavailable == Some(*member) {
                        Err("unavailable")
                    } else {
                        let value = self.next;
                        self.next += 1;
                        Ok(value)
                    }
                })
                .collect::<Vec<_>>();
            if self.short {
                results.pop();
            }
            Ok(results)
        }
        fn commit_component(&mut self, members: &[SymbolId]) {
            self.commits.push(members.to_vec());
        }
        fn rollback_component(&mut self, members: &[SymbolId]) {
            self.rollbacks.push(members.to_vec());
        }
    }

    #[test]
    fn combined_user_edge_merges_library_components_atomically() {
        let a = SymbolId(0);
        let b = SymbolId(1);
        let mut coordinator = ProjectLocalSemanticCoordinator::new([a, b], [(a, b), (b, a)]);
        let mut publisher = Publisher::default();
        coordinator.demand(a, &mut publisher).expect("demand");
        coordinator.force_all(&mut publisher).expect("force all");
        assert_eq!(publisher.begins, vec![vec![a, b]]);
        assert_eq!(publisher.commits, vec![vec![a, b]]);
        assert!(publisher.rollbacks.is_empty());
    }

    #[test]
    fn unavailable_rolls_back_the_whole_component() {
        let a = SymbolId(0);
        let b = SymbolId(1);
        let mut coordinator = ProjectLocalSemanticCoordinator::new([a, b], [(a, b), (b, a)]);
        let mut publisher = Publisher {
            unavailable: Some(b),
            ..Publisher::default()
        };
        coordinator.demand(a, &mut publisher).expect("demand");
        assert_eq!(publisher.rollbacks, vec![vec![a, b]]);
        assert_eq!(
            coordinator.observe(a),
            ProjectLocalObservation::Unavailable(&"unavailable")
        );
        assert_eq!(
            coordinator.observe(b),
            ProjectLocalObservation::Unavailable(&"unavailable")
        );
    }

    #[test]
    fn repeated_ready_demand_keeps_the_published_identity() {
        let group = SymbolId(0);
        let mut coordinator = ProjectLocalSemanticCoordinator::new([group], []);
        let mut publisher = Publisher::default();

        assert_eq!(
            coordinator.demand(group, &mut publisher),
            Ok(ProjectLocalObservation::Ready(&0))
        );
        assert_eq!(
            coordinator.demand(group, &mut publisher),
            Ok(ProjectLocalObservation::Ready(&0))
        );
        assert_eq!(publisher.begins, vec![vec![group]]);
        assert_eq!(publisher.commits, vec![vec![group]]);
    }

    #[test]
    fn dependency_components_publish_before_the_demanded_component() {
        let owner = SymbolId(0);
        let dependency = SymbolId(1);
        let mut coordinator =
            ProjectLocalSemanticCoordinator::new([owner, dependency], [(owner, dependency)]);
        let mut publisher = Publisher::default();

        coordinator.demand(owner, &mut publisher).expect("demand");

        assert_eq!(publisher.begins, vec![vec![dependency], vec![owner]]);
        assert_eq!(publisher.commits, vec![vec![dependency], vec![owner]]);

        let mut coordinator =
            ProjectLocalSemanticCoordinator::new([owner, dependency], [(owner, dependency)]);
        let mut publisher = Publisher::default();
        coordinator
            .demand(dependency, &mut publisher)
            .expect("dependency demand");
        assert_eq!(publisher.begins, vec![vec![dependency]]);
        assert_eq!(
            coordinator.observe(owner),
            ProjectLocalObservation::Unresolved
        );
    }

    #[test]
    fn publisher_failure_rolls_back_without_caching_resolving() {
        let group = SymbolId(0);
        let mut coordinator = ProjectLocalSemanticCoordinator::new([group], []);
        let mut publisher = Publisher {
            fail: true,
            ..Publisher::default()
        };

        assert_eq!(
            coordinator.demand(group, &mut publisher),
            Err(ProjectLocalDemandError::Publisher("publisher failure"))
        );
        assert_eq!(
            coordinator.observe(group),
            ProjectLocalObservation::Unresolved
        );
        assert_eq!(publisher.rollbacks, vec![vec![group]]);

        publisher.fail = false;
        assert_eq!(
            coordinator.demand(group, &mut publisher),
            Ok(ProjectLocalObservation::Ready(&0))
        );
    }

    #[test]
    fn cardinality_failure_rolls_back_and_can_retry() {
        let group = SymbolId(0);
        let mut coordinator = ProjectLocalSemanticCoordinator::new([group], []);
        let mut publisher = Publisher {
            short: true,
            ..Publisher::default()
        };

        assert_eq!(
            coordinator.demand(group, &mut publisher),
            Err(ProjectLocalDemandError::PublisherCardinality {
                expected: 1,
                actual: 0,
            })
        );
        assert_eq!(
            coordinator.observe(group),
            ProjectLocalObservation::Unresolved
        );
        publisher.short = false;
        coordinator.demand(group, &mut publisher).expect("retry");
        assert!(matches!(
            coordinator.observe(group),
            ProjectLocalObservation::Ready(_)
        ));
    }

    #[test]
    fn dependency_permutation_has_one_deterministic_partition_and_order() {
        let a = SymbolId(0);
        let b = SymbolId(1);
        let c = SymbolId(2);
        let edges = [(a, b), (b, a), (c, a)];
        let mut forward = ProjectLocalSemanticCoordinator::new([a, b, c], edges);
        let mut reverse = ProjectLocalSemanticCoordinator::new([c, a, b], edges.into_iter().rev());
        let mut forward_publisher = Publisher::default();
        let mut reverse_publisher = Publisher::default();

        forward
            .demand(c, &mut forward_publisher)
            .expect("forward demand");
        reverse
            .demand(c, &mut reverse_publisher)
            .expect("reverse demand");
        assert_eq!(forward_publisher.begins, reverse_publisher.begins);
        assert_eq!(forward_publisher.commits, reverse_publisher.commits);
    }

    #[test]
    fn perturbation_symbol_is_rejected_and_force_all_covers_only_admitted_owners() {
        let dependency = SymbolId(10);
        let owner = SymbolId(42);
        let perturbation = SymbolId(11);
        let mut coordinator =
            ProjectLocalSemanticCoordinator::new([owner, dependency], [(owner, dependency)]);
        let mut publisher = Publisher::default();

        assert_eq!(
            coordinator.demand(perturbation, &mut publisher),
            Err(ProjectLocalDemandError::UnknownGroup(perturbation))
        );
        coordinator.force_all(&mut publisher).expect("force all");

        let published = publisher
            .begins
            .iter()
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(published, vec![dependency, owner]);
        assert!(!published.contains(&perturbation));
    }
}

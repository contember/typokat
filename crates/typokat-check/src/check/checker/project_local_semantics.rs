//! Project-owned demand scheduling for immutable library declaration products.

#[cfg(test)]
use std::cell::Cell;
use std::collections::BTreeMap;

use crate::binder::namespace::LibraryProductBinderMap;
use crate::binder::symbol::SymbolId;
use crate::binder::Binder;
use crate::check::checker::decls::inventory::BoundTypeDeclarationInventory;
use oxc_ast::ast::TSTypeName;
use oxc_ast_visit::{walk, Visit};
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

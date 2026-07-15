//! Source declaration identities and dormant type-group metadata.

use crate::binder::namespace::NamespaceId;
use crate::binder::scope::ScopeId;
use crate::span::Span;
use oxc_ast::ast::{Program, TSModuleDeclarationName};
use oxc_ast::AstKind;
use oxc_ast_visit::Visit;

/// Unified lexical identity of one source declaration occurrence.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct DeclId(pub u32);

impl DeclId {
    #[inline]
    pub fn index(self) -> usize {
        usize::try_from(self.0).expect("lexical declaration id fits usize")
    }
}

/// Checker storage identity for a value declaration's computed type.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ValueStorageId(pub u32);

impl ValueStorageId {
    #[inline]
    pub fn index(self) -> usize {
        usize::try_from(self.0).expect("value storage id fits usize")
    }
}

/// Checker storage identity retained by the pre-group production type path.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct LegacyTypeStorageId(pub u32);

impl LegacyTypeStorageId {
    #[inline]
    pub fn index(self) -> usize {
        usize::try_from(self.0).expect("legacy type storage id fits usize")
    }
}

/// Stable identity of an ordered same-name type declaration group.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct TypeGroupId(pub u32);

impl TypeGroupId {
    #[inline]
    pub fn index(self) -> usize {
        usize::try_from(self.0).expect("type group id fits usize")
    }
}

/// The source form that introduced one lexical declaration.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum DeclarationKind {
    Variable,
    Function,
    Class,
    Parameter,
    CatchParameter,
    Import,
    TypeAlias,
    Interface,
    Enum,
    Namespace,
    ImportEquals,
    NamespaceExport,
    Global,
}

/// Exact AST node and binding-leaf site of one source declaration occurrence.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct DeclarationSite {
    pub module: ScopeId,
    /// Production lexical scope, attached only when the semantic binder visits this occurrence.
    pub scope: Option<ScopeId>,
    pub declaration_span: Span,
    pub binding_span: Span,
}

/// Source-only declaration occurrence found independently of semantic support.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) struct SourceDeclarationOccurrence {
    pub kind: DeclarationKind,
    pub declaration_span: Span,
    pub binding_span: Span,
}

/// Walk every source declaration occurrence without consulting binder semantics.
pub(crate) fn source_declaration_occurrences(
    program: &Program<'_>,
) -> Vec<SourceDeclarationOccurrence> {
    let mut visitor = SourceDeclarationVisitor::default();
    visitor.visit_program(program);
    visitor.occurrences
}

#[derive(Default)]
struct SourceDeclarationVisitor {
    occurrences: Vec<SourceDeclarationOccurrence>,
}

impl SourceDeclarationVisitor {
    fn push(
        &mut self,
        kind: DeclarationKind,
        declaration_span: oxc_span::Span,
        binding_span: oxc_span::Span,
    ) {
        self.occurrences.push(SourceDeclarationOccurrence {
            kind,
            declaration_span: Span::from_oxc(declaration_span),
            binding_span: Span::from_oxc(binding_span),
        });
    }

    fn push_pattern(
        &mut self,
        kind: DeclarationKind,
        declaration_span: oxc_span::Span,
        pattern: &oxc_ast::ast::BindingPattern<'_>,
    ) {
        for identifier in pattern.get_binding_identifiers() {
            self.push(kind, declaration_span, identifier.span);
        }
    }
}

impl<'a> Visit<'a> for SourceDeclarationVisitor {
    fn enter_node(&mut self, kind: AstKind<'a>) {
        match kind {
            AstKind::VariableDeclarator(declaration) => {
                self.push_pattern(DeclarationKind::Variable, declaration.span, &declaration.id)
            }
            AstKind::Function(function) => {
                if let Some(identifier) = &function.id {
                    self.push(DeclarationKind::Function, function.span, identifier.span);
                }
            }
            AstKind::Class(class) => {
                if let Some(identifier) = &class.id {
                    self.push(DeclarationKind::Class, class.span, identifier.span);
                }
            }
            AstKind::FormalParameter(parameter) => self.push_pattern(
                DeclarationKind::Parameter,
                parameter.span,
                &parameter.pattern,
            ),
            AstKind::FormalParameterRest(parameter) => self.push_pattern(
                DeclarationKind::Parameter,
                parameter.span,
                &parameter.rest.argument,
            ),
            AstKind::CatchClause(clause) => {
                if let Some(parameter) = &clause.param {
                    self.push_pattern(
                        DeclarationKind::CatchParameter,
                        parameter.span,
                        &parameter.pattern,
                    );
                }
            }
            AstKind::ImportDeclaration(declaration) => {
                if let Some(specifiers) = &declaration.specifiers {
                    for specifier in specifiers {
                        self.push(
                            DeclarationKind::Import,
                            declaration.span,
                            specifier.local().span,
                        );
                    }
                }
            }
            AstKind::TSTypeAliasDeclaration(declaration) => self.push(
                DeclarationKind::TypeAlias,
                declaration.span,
                declaration.id.span,
            ),
            AstKind::TSInterfaceDeclaration(declaration) => self.push(
                DeclarationKind::Interface,
                declaration.span,
                declaration.id.span,
            ),
            AstKind::TSEnumDeclaration(declaration) => {
                self.push(DeclarationKind::Enum, declaration.span, declaration.id.span)
            }
            AstKind::TSModuleDeclaration(declaration) => {
                let binding_span = match &declaration.id {
                    TSModuleDeclarationName::Identifier(identifier) => identifier.span,
                    TSModuleDeclarationName::StringLiteral(literal) => literal.span,
                };
                self.push(DeclarationKind::Namespace, declaration.span, binding_span);
            }
            AstKind::TSImportEqualsDeclaration(declaration) => self.push(
                DeclarationKind::ImportEquals,
                declaration.span,
                declaration.id.span,
            ),
            AstKind::TSNamespaceExportDeclaration(declaration) => self.push(
                DeclarationKind::NamespaceExport,
                declaration.span,
                declaration.id.span,
            ),
            AstKind::TSGlobalDeclaration(declaration) => self.push(
                DeclarationKind::Global,
                declaration.span,
                declaration.global_span,
            ),
            _ => {}
        }
    }
}

/// One source occurrence and its independent checker storage identities.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LexicalDeclaration {
    pub id: DeclId,
    pub kind: DeclarationKind,
    pub site: DeclarationSite,
    pub value_storage: Option<ValueStorageId>,
    pub legacy_type_storage: Option<LegacyTypeStorageId>,
    pub type_group: Option<TypeGroupId>,
    /// Dormant exact namespace identity for namespace headers.
    pub namespace: Option<NamespaceId>,
}

/// Dense declaration table indexed only by unified lexical [`DeclId`].
#[derive(Default)]
pub struct DeclarationTable {
    declarations: Vec<LexicalDeclaration>,
}

impl DeclarationTable {
    pub(crate) fn push(&mut self, kind: DeclarationKind, site: DeclarationSite) -> DeclId {
        let id = DeclId(
            u32::try_from(self.declarations.len()).expect("declaration table length fits u32"),
        );
        self.declarations.push(LexicalDeclaration {
            id,
            kind,
            site,
            value_storage: None,
            legacy_type_storage: None,
            type_group: None,
            namespace: None,
        });
        id
    }

    pub fn get(&self, id: DeclId) -> Option<&LexicalDeclaration> {
        self.declarations.get(id.index())
    }

    pub(crate) fn get_mut(&mut self, id: DeclId) -> Option<&mut LexicalDeclaration> {
        self.declarations.get_mut(id.index())
    }

    pub fn iter(&self) -> impl Iterator<Item = &LexicalDeclaration> {
        self.declarations.iter()
    }

    pub fn len(&self) -> usize {
        self.declarations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.declarations.is_empty()
    }
}

/// Type-bearing source form retained in an ordered dormant group.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum TypeFragmentKind {
    TypeAlias,
    Interface,
    Class,
}

/// One ordered group fragment with its exact source and legacy storage sites.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct TypeGroupFragment {
    pub declaration: DeclId,
    pub scope: ScopeId,
    pub site: DeclarationSite,
    pub kind: TypeFragmentKind,
    pub legacy_storage: LegacyTypeStorageId,
}

/// Dormant ordered metadata for every admitted same-name type declaration.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TypeGroup {
    pub id: TypeGroupId,
    pub name: String,
    pub fragments: Vec<TypeGroupFragment>,
}

/// Dense stable group table indexed by [`TypeGroupId`].
#[derive(Default)]
pub struct TypeGroupTable {
    groups: Vec<TypeGroup>,
}

impl TypeGroupTable {
    pub(crate) fn push(&mut self, name: impl Into<String>) -> TypeGroupId {
        let id = TypeGroupId(
            u32::try_from(self.groups.len()).expect("type group table length fits u32"),
        );
        self.groups.push(TypeGroup {
            id,
            name: name.into(),
            fragments: Vec::new(),
        });
        id
    }

    pub fn get(&self, id: TypeGroupId) -> Option<&TypeGroup> {
        self.groups.get(id.index())
    }

    pub(crate) fn get_mut(&mut self, id: TypeGroupId) -> Option<&mut TypeGroup> {
        self.groups.get_mut(id.index())
    }

    pub fn len(&self) -> usize {
        self.groups.len()
    }

    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    #[test]
    fn visitor_inventory_covers_every_lexical_declaration_variant() {
        let source = "import Default, { named as Local } from './dep'; declare const variable: number; declare function callable(param: number): void; declare class Klass {} type Alias = number; interface Shape {} enum Choice {} declare namespace Space {} declare module 'pkg' {} declare global {} import Equal = require('pkg'); export as namespace Published;";
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::d_ts()).parse();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let occurrences = source_declaration_occurrences(&parsed.program);

        assert_eq!(
            occurrences
                .iter()
                .map(|occurrence| occurrence.kind)
                .collect::<Vec<_>>(),
            vec![
                DeclarationKind::Import,
                DeclarationKind::Import,
                DeclarationKind::Variable,
                DeclarationKind::Function,
                DeclarationKind::Parameter,
                DeclarationKind::Class,
                DeclarationKind::TypeAlias,
                DeclarationKind::Interface,
                DeclarationKind::Enum,
                DeclarationKind::Namespace,
                DeclarationKind::Namespace,
                DeclarationKind::Global,
                DeclarationKind::ImportEquals,
                DeclarationKind::NamespaceExport,
            ]
        );
        let binding_names: Vec<_> = occurrences
            .iter()
            .map(|occurrence| &source[occurrence.binding_span.range()])
            .collect();
        assert_eq!(
            binding_names,
            vec![
                "Default",
                "Local",
                "variable",
                "callable",
                "param",
                "Klass",
                "Alias",
                "Shape",
                "Choice",
                "Space",
                "'pkg'",
                "global",
                "Equal",
                "Published",
            ]
        );
        for (kind, declaration_text, binding_text) in [
            (
                DeclarationKind::ImportEquals,
                "import Equal = require('pkg');",
                "Equal",
            ),
            (
                DeclarationKind::NamespaceExport,
                "export as namespace Published;",
                "Published",
            ),
            (DeclarationKind::Global, "declare global {}", "global"),
        ] {
            let occurrence = occurrences
                .iter()
                .find(|occurrence| occurrence.kind == kind)
                .expect("inventory declaration");
            assert_eq!(
                &source[occurrence.declaration_span.range()],
                declaration_text
            );
            assert_eq!(&source[occurrence.binding_span.range()], binding_text);
        }
    }

    #[test]
    fn complete_visitor_reaches_declarations_inside_unmodeled_expressions() {
        let source = "tag`${(function nested({ leaf: [deep] }) {})}`;";
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let occurrences = source_declaration_occurrences(&parsed.program);
        assert_eq!(
            occurrences
                .iter()
                .map(|occurrence| &source[occurrence.binding_span.range()])
                .collect::<Vec<_>>(),
            vec!["nested", "deep"]
        );
    }
}

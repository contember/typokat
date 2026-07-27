//! Source declaration identities and dormant type-group metadata.

use crate::binder::namespace::{ModuleBindingContext, NamespaceId, SourceUnitKey};
use crate::binder::scope::ScopeId;
use crate::span::Span;
use crate::types::layered::{LayeredMap, LayeredVec};
use oxc_ast::ast::{
    ClassType, Declaration, ImportOrExportKind, ModuleDeclaration, Program, Statement,
    TSModuleDeclarationName, VariableDeclarationKind,
};
use oxc_ast::AstKind;
use oxc_ast_visit::{walk, Visit};
#[cfg(test)]
use rustc_hash::FxHashMap;
use std::collections::{BTreeMap, BTreeSet};

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

/// Stable identity of an ordered same-name type declaration group.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
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

/// Global binding projection from the exhaustive visitor shared with semantic binding.
pub(crate) fn source_global_binding_census(
    program: &Program<'_>,
    context: ModuleBindingContext,
) -> SourceGlobalBindingCensus {
    let mut visitor = SourceDeclarationVisitor::with_global_census(context);
    visitor.visit_program(program);
    visitor
        .global_census
        .expect("global census projection is enabled")
        .result
}

pub(crate) fn source_global_binding_census_with_provenance(
    program: &Program<'_>,
    context: ModuleBindingContext,
) -> SourceGlobalBindingProvenance {
    let mut visitor = SourceDeclarationVisitor::with_global_census_provenance(context);
    visitor.visit_program(program);
    let projection = visitor
        .global_census
        .expect("global census provenance projection is enabled");
    SourceGlobalBindingProvenance {
        census: projection.result,
        binding_sites: projection
            .binding_sites
            .expect("binding-site provenance is enabled"),
        contributor_sites: projection
            .contributor_sites
            .expect("contributor provenance is enabled"),
        explicit_global_this_sites: projection
            .explicit_global_this_sites
            .expect("globalThis provenance is enabled"),
    }
}

pub(crate) fn source_declaration_occurrences(
    program: &Program<'_>,
) -> Vec<SourceDeclarationOccurrence> {
    let mut visitor = SourceDeclarationVisitor::occurrences_only();
    visitor.visit_program(program);
    visitor
        .occurrences
        .expect("declaration occurrence projection is enabled")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SourceBindingSlot {
    Value,
    Type,
    Namespace,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SourceGlobalBindingCandidate {
    pub(crate) slots: BTreeSet<SourceBindingSlot>,
    pub(crate) global_object_contributor: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SourceGlobalBindingCensus {
    pub(crate) candidates: BTreeMap<String, SourceGlobalBindingCandidate>,
    pub(crate) uncertain_candidates: BTreeMap<String, SourceGlobalBindingCandidate>,
    pub(crate) explicit_global_this: bool,
    pub(crate) umd_global: bool,
    pub(crate) uncertain_relevant_syntax: bool,
    pub(crate) source_nodes_visited: u64,
    pub(crate) binding_leaves_visited: u64,
}

pub(crate) struct SourceGlobalBindingProvenance {
    pub(crate) census: SourceGlobalBindingCensus,
    pub(crate) binding_sites: Vec<SourceGlobalBindingSite>,
    pub(crate) contributor_sites: Vec<SourceGlobalContributorSite>,
    pub(crate) explicit_global_this_sites: Vec<Span>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceGlobalBindingSite {
    pub(crate) name: String,
    pub(crate) span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceGlobalContributorKind {
    Ordinary,
    Namespace,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceGlobalContributorSite {
    pub(crate) name: String,
    pub(crate) kind: SourceGlobalContributorKind,
    pub(crate) span: Span,
}

struct SourceDeclarationVisitor {
    occurrences: Option<Vec<SourceDeclarationOccurrence>>,
    global_census: Option<SourceGlobalCensusProjection>,
}

struct SourceGlobalCensusProjection {
    result: SourceGlobalBindingCensus,
    context: ModuleBindingContext,
    function_depth: usize,
    class_depth: usize,
    module_depth: usize,
    statement_nesting_depth: usize,
    variable_kinds: Vec<VariableDeclarationKind>,
    global_boundaries: Vec<(usize, usize, usize, usize, GlobalBoundaryDisposition)>,
    binding_sites: Option<Vec<SourceGlobalBindingSite>>,
    contributor_sites: Option<Vec<SourceGlobalContributorSite>>,
    explicit_global_this_sites: Option<Vec<Span>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GlobalBoundaryDisposition {
    Legal,
    DirectScriptRejected,
    NestedUncertain,
}

impl SourceDeclarationVisitor {
    fn occurrences_only() -> Self {
        Self {
            occurrences: Some(Vec::new()),
            global_census: None,
        }
    }

    fn with_global_census(context: ModuleBindingContext) -> Self {
        Self {
            occurrences: None,
            global_census: Some(SourceGlobalCensusProjection {
                result: SourceGlobalBindingCensus::default(),
                context,
                function_depth: 0,
                class_depth: 0,
                module_depth: 0,
                statement_nesting_depth: 0,
                variable_kinds: Vec::new(),
                global_boundaries: Vec::new(),
                binding_sites: None,
                contributor_sites: None,
                explicit_global_this_sites: None,
            }),
        }
    }

    fn with_global_census_provenance(context: ModuleBindingContext) -> Self {
        let mut visitor = Self::with_global_census(context);
        let projection = visitor
            .global_census
            .as_mut()
            .expect("global census projection is enabled");
        projection.binding_sites = Some(Vec::new());
        projection.contributor_sites = Some(Vec::new());
        projection.explicit_global_this_sites = Some(Vec::new());
        visitor
    }

    fn push(
        &mut self,
        kind: DeclarationKind,
        declaration_span: oxc_span::Span,
        binding_span: oxc_span::Span,
    ) {
        if let Some(occurrences) = self.occurrences.as_mut() {
            occurrences.push(SourceDeclarationOccurrence {
                kind,
                declaration_span: Span::from_oxc(declaration_span),
                binding_span: Span::from_oxc(binding_span),
            });
        }
    }

    fn push_pattern(
        &mut self,
        kind: DeclarationKind,
        declaration_span: oxc_span::Span,
        pattern: &oxc_ast::ast::BindingPattern<'_>,
    ) {
        let Some(occurrences) = self.occurrences.as_mut() else {
            return;
        };
        for identifier in pattern.get_binding_identifiers() {
            occurrences.push(SourceDeclarationOccurrence {
                kind,
                declaration_span: Span::from_oxc(declaration_span),
                binding_span: Span::from_oxc(identifier.span),
            });
        }
    }

    fn root_placement(&self) -> Option<GlobalRootPlacement> {
        let census = self.global_census.as_ref()?;
        if let Some(&(
            function_depth,
            class_depth,
            module_depth,
            statement_nesting_depth,
            disposition,
        )) = census.global_boundaries.last()
        {
            if disposition == GlobalBoundaryDisposition::DirectScriptRejected {
                return None;
            }
            if census.function_depth == function_depth
                && census.class_depth == class_depth
                && census.module_depth == module_depth
            {
                return Some(GlobalRootPlacement {
                    direct_lexical: census.statement_nesting_depth == statement_nesting_depth,
                    legal: disposition == GlobalBoundaryDisposition::Legal,
                });
            }
            return None;
        }
        (!census.context.external_module
            && census.function_depth == 0
            && census.class_depth == 0
            && census.module_depth == 0)
            .then_some(GlobalRootPlacement {
                direct_lexical: census.statement_nesting_depth == 0,
                legal: true,
            })
    }

    fn candidate(
        &mut self,
        name: &str,
        slots: &[SourceBindingSlot],
        contributor: Option<SourceGlobalContributorKind>,
        binding_span: oxc_span::Span,
    ) {
        let Some(census) = self.global_census.as_mut() else {
            return;
        };
        let uncertain = census
            .global_boundaries
            .last()
            .is_some_and(|(_, _, _, _, disposition)| {
                *disposition == GlobalBoundaryDisposition::NestedUncertain
            });
        let candidates = if uncertain {
            &mut census.result.uncertain_candidates
        } else {
            &mut census.result.candidates
        };
        let candidate = candidates.entry(name.to_owned()).or_default();
        candidate.slots.extend(slots.iter().copied());
        candidate.global_object_contributor |= contributor.is_some();
        if let Some(sites) = census.binding_sites.as_mut() {
            sites.push(SourceGlobalBindingSite {
                name: name.to_owned(),
                span: Span::from_oxc(binding_span),
            });
        }
        if !uncertain {
            if let Some(kind) = contributor {
                if let Some(sites) = census.contributor_sites.as_mut() {
                    sites.push(SourceGlobalContributorSite {
                        name: name.to_owned(),
                        kind,
                        span: Span::from_oxc(binding_span),
                    });
                }
            }
        }
        census.result.binding_leaves_visited =
            census.result.binding_leaves_visited.saturating_add(1);
    }

    fn candidate_pattern(
        &mut self,
        pattern: &oxc_ast::ast::BindingPattern<'_>,
        global_object_contributor: bool,
    ) {
        for identifier in pattern.get_binding_identifiers() {
            self.candidate(
                identifier.name.as_str(),
                &[SourceBindingSlot::Value],
                global_object_contributor.then_some(SourceGlobalContributorKind::Ordinary),
                identifier.span,
            );
        }
    }

    fn global_placement_is_legal(&self) -> bool {
        let Some(census) = self.global_census.as_ref() else {
            return false;
        };
        census.context.external_module
            && census.global_boundaries.is_empty()
            && census.function_depth == 0
            && census.class_depth == 0
            && census.module_depth == 0
            && census.statement_nesting_depth == 0
    }

    fn direct_script_global_is_rejected(&self) -> bool {
        self.global_census.as_ref().is_some_and(|census| {
            !census.context.external_module
                && census.global_boundaries.is_empty()
                && census.function_depth == 0
                && census.class_depth == 0
                && census.module_depth == 0
                && census.statement_nesting_depth == 0
        })
    }
}

#[derive(Clone, Copy)]
struct GlobalRootPlacement {
    direct_lexical: bool,
    legal: bool,
}

impl<'a> Visit<'a> for SourceDeclarationVisitor {
    fn visit_statement(&mut self, statement: &Statement<'a>) {
        let nested_placement = match statement {
            Statement::BlockStatement(_)
            | Statement::DoWhileStatement(_)
            | Statement::ForInStatement(_)
            | Statement::ForOfStatement(_)
            | Statement::ForStatement(_)
            | Statement::IfStatement(_)
            | Statement::LabeledStatement(_)
            | Statement::SwitchStatement(_)
            | Statement::TryStatement(_)
            | Statement::WhileStatement(_)
            | Statement::WithStatement(_) => true,
            Statement::BreakStatement(_)
            | Statement::ContinueStatement(_)
            | Statement::DebuggerStatement(_)
            | Statement::EmptyStatement(_)
            | Statement::ExpressionStatement(_)
            | Statement::ReturnStatement(_)
            | Statement::ThrowStatement(_)
            | Statement::VariableDeclaration(_)
            | Statement::FunctionDeclaration(_)
            | Statement::ClassDeclaration(_)
            | Statement::TSTypeAliasDeclaration(_)
            | Statement::TSInterfaceDeclaration(_)
            | Statement::TSEnumDeclaration(_)
            | Statement::TSModuleDeclaration(_)
            | Statement::TSGlobalDeclaration(_)
            | Statement::TSImportEqualsDeclaration(_)
            | Statement::ImportDeclaration(_)
            | Statement::ExportAllDeclaration(_)
            | Statement::ExportDefaultDeclaration(_)
            | Statement::ExportNamedDeclaration(_)
            | Statement::TSExportAssignment(_)
            | Statement::TSNamespaceExportDeclaration(_) => false,
        };
        if nested_placement {
            if let Some(census) = self.global_census.as_mut() {
                census.statement_nesting_depth = census.statement_nesting_depth.saturating_add(1);
            }
        }
        walk::walk_statement(self, statement);
        if nested_placement {
            if let Some(census) = self.global_census.as_mut() {
                census.statement_nesting_depth = census.statement_nesting_depth.saturating_sub(1);
            }
        }
    }

    fn visit_declaration(&mut self, declaration: &Declaration<'a>) {
        match declaration {
            Declaration::VariableDeclaration(_)
            | Declaration::FunctionDeclaration(_)
            | Declaration::ClassDeclaration(_)
            | Declaration::TSTypeAliasDeclaration(_)
            | Declaration::TSInterfaceDeclaration(_)
            | Declaration::TSEnumDeclaration(_)
            | Declaration::TSModuleDeclaration(_)
            | Declaration::TSGlobalDeclaration(_)
            | Declaration::TSImportEqualsDeclaration(_) => {}
        }
        walk::walk_declaration(self, declaration);
    }

    fn visit_module_declaration(&mut self, declaration: &ModuleDeclaration<'a>) {
        match declaration {
            ModuleDeclaration::ImportDeclaration(_)
            | ModuleDeclaration::ExportAllDeclaration(_)
            | ModuleDeclaration::ExportDefaultDeclaration(_)
            | ModuleDeclaration::ExportNamedDeclaration(_)
            | ModuleDeclaration::TSExportAssignment(_)
            | ModuleDeclaration::TSNamespaceExportDeclaration(_) => {}
        }
        walk::walk_module_declaration(self, declaration);
    }

    fn enter_node(&mut self, kind: AstKind<'a>) {
        if let Some(census) = self.global_census.as_mut() {
            census.result.source_nodes_visited =
                census.result.source_nodes_visited.saturating_add(1);
        }
        match kind {
            AstKind::VariableDeclaration(declaration) => {
                if let Some(census) = self.global_census.as_mut() {
                    census.variable_kinds.push(declaration.kind);
                }
            }
            AstKind::VariableDeclarator(declaration) => {
                self.push_pattern(DeclarationKind::Variable, declaration.span, &declaration.id);
                if let Some(root) = self.root_placement() {
                    let kind = self
                        .global_census
                        .as_ref()
                        .and_then(|census| census.variable_kinds.last().copied());
                    let admitted =
                        root.direct_lexical || kind.is_some_and(VariableDeclarationKind::is_var);
                    if admitted {
                        let contributor = kind.is_some_and(VariableDeclarationKind::is_var);
                        self.candidate_pattern(&declaration.id, contributor);
                    }
                }
            }
            AstKind::Function(function) => {
                if let Some(identifier) = &function.id {
                    self.push(DeclarationKind::Function, function.span, identifier.span);
                    if function.is_declaration()
                        && self
                            .root_placement()
                            .is_some_and(|placement| placement.direct_lexical)
                    {
                        self.candidate(
                            identifier.name.as_str(),
                            &[SourceBindingSlot::Value],
                            Some(SourceGlobalContributorKind::Ordinary),
                            identifier.span,
                        );
                    }
                }
                if let Some(census) = self.global_census.as_mut() {
                    census.function_depth = census.function_depth.saturating_add(1);
                }
            }
            AstKind::Class(class) => {
                if let Some(identifier) = &class.id {
                    self.push(DeclarationKind::Class, class.span, identifier.span);
                    if class.r#type == ClassType::ClassDeclaration
                        && self
                            .root_placement()
                            .is_some_and(|placement| placement.direct_lexical)
                    {
                        self.candidate(
                            identifier.name.as_str(),
                            &[SourceBindingSlot::Value, SourceBindingSlot::Type],
                            None,
                            identifier.span,
                        );
                    }
                }
                if let Some(census) = self.global_census.as_mut() {
                    census.class_depth = census.class_depth.saturating_add(1);
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
            AstKind::TSTypeAliasDeclaration(declaration) => {
                self.push(
                    DeclarationKind::TypeAlias,
                    declaration.span,
                    declaration.id.span,
                );
                if self
                    .root_placement()
                    .is_some_and(|placement| placement.direct_lexical)
                {
                    self.candidate(
                        declaration.id.name.as_str(),
                        &[SourceBindingSlot::Type],
                        None,
                        declaration.id.span,
                    );
                }
            }
            AstKind::TSInterfaceDeclaration(declaration) => {
                self.push(
                    DeclarationKind::Interface,
                    declaration.span,
                    declaration.id.span,
                );
                if self
                    .root_placement()
                    .is_some_and(|placement| placement.direct_lexical)
                {
                    self.candidate(
                        declaration.id.name.as_str(),
                        &[SourceBindingSlot::Type],
                        None,
                        declaration.id.span,
                    );
                }
            }
            AstKind::TSEnumDeclaration(declaration) => {
                self.push(DeclarationKind::Enum, declaration.span, declaration.id.span);
                if self
                    .root_placement()
                    .is_some_and(|placement| placement.direct_lexical)
                {
                    self.candidate(
                        declaration.id.name.as_str(),
                        &[SourceBindingSlot::Value, SourceBindingSlot::Type],
                        None,
                        declaration.id.span,
                    );
                }
            }
            AstKind::TSModuleDeclaration(declaration) => {
                let binding_span = match &declaration.id {
                    TSModuleDeclarationName::Identifier(identifier) => identifier.span,
                    TSModuleDeclarationName::StringLiteral(literal) => literal.span,
                };
                self.push(DeclarationKind::Namespace, declaration.span, binding_span);
                if let TSModuleDeclarationName::Identifier(identifier) = &declaration.id {
                    if let Some(placement) = self.root_placement() {
                        if placement.direct_lexical {
                            if identifier.name == "globalThis" && placement.legal {
                                let census = self
                                    .global_census
                                    .as_mut()
                                    .expect("root placement requires the census projection");
                                census.result.explicit_global_this = true;
                                if let Some(sites) = census.explicit_global_this_sites.as_mut() {
                                    sites.push(Span::from_oxc(identifier.span));
                                }
                            }
                            self.candidate(
                                identifier.name.as_str(),
                                &[SourceBindingSlot::Value, SourceBindingSlot::Namespace],
                                Some(SourceGlobalContributorKind::Namespace),
                                identifier.span,
                            );
                        }
                    }
                }
                if let Some(census) = self.global_census.as_mut() {
                    census.module_depth = census.module_depth.saturating_add(1);
                }
            }
            AstKind::TSImportEqualsDeclaration(declaration) => {
                self.push(
                    DeclarationKind::ImportEquals,
                    declaration.span,
                    declaration.id.span,
                );
                if self
                    .root_placement()
                    .is_some_and(|placement| placement.direct_lexical)
                {
                    let slots: &[SourceBindingSlot] = match declaration.import_kind {
                        ImportOrExportKind::Value => &[
                            SourceBindingSlot::Value,
                            SourceBindingSlot::Type,
                            SourceBindingSlot::Namespace,
                        ],
                        ImportOrExportKind::Type => &[SourceBindingSlot::Type],
                    };
                    self.candidate(
                        declaration.id.name.as_str(),
                        slots,
                        None,
                        declaration.id.span,
                    );
                }
            }
            AstKind::TSNamespaceExportDeclaration(declaration) => {
                self.push(
                    DeclarationKind::NamespaceExport,
                    declaration.span,
                    declaration.id.span,
                );
                if let Some(census) = self.global_census.as_mut() {
                    census.result.umd_global = true;
                }
            }
            AstKind::TSGlobalDeclaration(declaration) => {
                self.push(
                    DeclarationKind::Global,
                    declaration.span,
                    declaration.global_span,
                );
                let placement_is_legal = self.global_placement_is_legal();
                let direct_script_rejected = self.direct_script_global_is_rejected();
                if let Some(census) = self.global_census.as_mut() {
                    if !placement_is_legal {
                        census.result.uncertain_relevant_syntax = true;
                    }
                    let disposition = if placement_is_legal {
                        GlobalBoundaryDisposition::Legal
                    } else if direct_script_rejected {
                        GlobalBoundaryDisposition::DirectScriptRejected
                    } else {
                        GlobalBoundaryDisposition::NestedUncertain
                    };
                    census.global_boundaries.push((
                        census.function_depth,
                        census.class_depth,
                        census.module_depth,
                        census.statement_nesting_depth,
                        disposition,
                    ));
                }
            }
            AstKind::ArrowFunctionExpression(_) => {
                if let Some(census) = self.global_census.as_mut() {
                    census.function_depth = census.function_depth.saturating_add(1);
                }
            }
            _ => {}
        }
    }

    fn leave_node(&mut self, kind: AstKind<'a>) {
        let Some(census) = self.global_census.as_mut() else {
            return;
        };
        match kind {
            AstKind::VariableDeclaration(_) => {
                census.variable_kinds.pop();
            }
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                census.function_depth = census.function_depth.saturating_sub(1);
            }
            AstKind::Class(_) => {
                census.class_depth = census.class_depth.saturating_sub(1);
            }
            AstKind::TSModuleDeclaration(_) => {
                census.module_depth = census.module_depth.saturating_sub(1);
            }
            AstKind::TSGlobalDeclaration(_) => {
                census.global_boundaries.pop();
            }
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
    pub type_group: Option<TypeGroupId>,
    /// Dormant exact namespace identity for namespace headers.
    pub namespace: Option<NamespaceId>,
}

/// Dense canonical declaration rows with exact-site lookup into unified lexical [`DeclId`].
#[derive(Default)]
pub struct DeclarationTable {
    declarations: LayeredVec<LexicalDeclaration>,
    declarations_by_site: LayeredMap<(ScopeId, u32, DeclarationKind), DeclId>,
}

impl DeclarationTable {
    pub(crate) fn push(&mut self, kind: DeclarationKind, site: DeclarationSite) -> DeclId {
        let id = DeclId(
            u32::try_from(self.declarations.len()).expect("declaration table length fits u32"),
        );
        self.declarations.push_local(LexicalDeclaration {
            id,
            kind,
            site,
            value_storage: None,
            type_group: None,
            namespace: None,
        });
        let previous = self
            .declarations_by_site
            .insert_local((site.module, site.binding_span.start, kind), id);
        debug_assert!(
            matches!(previous, Ok(None)),
            "one declaration per binding leaf"
        );
        id
    }

    pub(crate) fn declaration_at_site(
        &self,
        syntax_module: ScopeId,
        binding_start: u32,
        kind: DeclarationKind,
    ) -> Option<&LexicalDeclaration> {
        let declaration = self
            .declarations_by_site
            .get(&(syntax_module, binding_start, kind))?;
        self.declarations.get(declaration.index())
    }

    pub fn get(&self, id: DeclId) -> Option<&LexicalDeclaration> {
        self.declarations.get(id.index())
    }

    pub(crate) fn get_mut(&mut self, id: DeclId) -> Option<&mut LexicalDeclaration> {
        self.declarations.get_mut_local(id.index())
    }

    pub fn iter(&self) -> impl Iterator<Item = &LexicalDeclaration> {
        self.declarations.iter()
    }

    pub(crate) fn local_declarations(&self) -> impl Iterator<Item = &LexicalDeclaration> {
        self.declarations.local_iter()
    }

    pub fn len(&self) -> usize {
        self.declarations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.declarations.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn from_snapshot(
        declarations: Vec<LexicalDeclaration>,
    ) -> Result<Self, &'static str> {
        let mut declarations_by_site = FxHashMap::default();
        for (index, declaration) in declarations.iter().enumerate() {
            if declaration.id.index() != index {
                return Err("snapshot declaration ids are not dense");
            }
            let key = (
                declaration.site.module,
                declaration.site.binding_span.start,
                declaration.kind,
            );
            if declarations_by_site.insert(key, declaration.id).is_some() {
                return Err("snapshot declaration-site index contains a duplicate");
            }
        }
        let mut table = Self::default();
        for declaration in declarations {
            table.declarations.push_local(declaration);
        }
        for (key, declaration) in declarations_by_site {
            table.declarations_by_site.insert_local(key, declaration)?;
        }
        Ok(table)
    }

    pub(crate) fn freeze_as_base(&mut self) -> Result<(), &'static str> {
        self.declarations.freeze_as_base()?;
        self.declarations_by_site.freeze_as_base()
    }

    pub(crate) fn fork_delta(&self) -> Result<Self, &'static str> {
        Ok(Self {
            declarations: self.declarations.fork_delta()?,
            declarations_by_site: self.declarations_by_site.fork_delta()?,
        })
    }

    #[cfg(test)]
    pub(crate) fn shares_base_storage_with(&self, other: &Self) -> bool {
        self.declarations.shares_base_with(&other.declarations)
            && self
                .declarations_by_site
                .shares_base_with(&other.declarations_by_site)
    }

    #[cfg(test)]
    pub(crate) fn base_family_sharing_with(&self, other: &Self) -> [bool; 2] {
        [
            self.declarations.shares_base_with(&other.declarations),
            self.declarations_by_site
                .shares_base_with(&other.declarations_by_site),
        ]
    }

    #[cfg(test)]
    pub(crate) fn local_family_row_counts_for_test(&self) -> [usize; 2] {
        [
            self.declarations.local_len(),
            self.declarations_by_site.local_len(),
        ]
    }
}

/// Type-bearing source form retained in an ordered dormant group.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum TypeFragmentKind {
    TypeAlias,
    Interface,
    Class,
}

/// One ordered group fragment with its exact lexical scope.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct TypeGroupFragment {
    pub declaration: DeclId,
    pub(crate) source: SourceUnitKey,
    pub scope: ScopeId,
    pub site: DeclarationSite,
    pub kind: TypeFragmentKind,
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
    groups: LayeredVec<TypeGroup>,
}

/// Where [`TypeGroupTable::append_fragment`] put a fragment.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) struct AppendedFragment {
    pub(crate) group: TypeGroupId,
    /// The requested group belongs to the frozen prefix, so the fragment went to a fresh
    /// delta-local group instead.
    pub(crate) frozen_merge_refused: bool,
}

impl TypeGroupTable {
    fn push_with_fragment(&mut self, name: &str, fragment: TypeGroupFragment) -> TypeGroupId {
        let id = TypeGroupId(
            u32::try_from(self.groups.len()).expect("type group table length fits u32"),
        );
        self.groups.push_local(TypeGroup {
            id,
            name: name.to_owned(),
            fragments: vec![fragment],
        });
        id
    }

    /// Append `fragment` to `group`, or to a fresh delta-local group when there is none yet.
    ///
    /// A group inside the frozen library prefix can never take another fragment (ADR-0011), so
    /// the merge is refused there and the fragment gets a group of its own; the caller records
    /// the refusal rather than mutating a base row (backlog 103).
    pub(crate) fn append_fragment(
        &mut self,
        group: Option<TypeGroupId>,
        name: &str,
        fragment: TypeGroupFragment,
    ) -> AppendedFragment {
        if let Some(id) = group {
            if let Some(row) = self.groups.get_mut_local(id.index()) {
                row.fragments.push(fragment);
                return AppendedFragment {
                    group: id,
                    frozen_merge_refused: false,
                };
            }
            return AppendedFragment {
                group: self.push_with_fragment(name, fragment),
                frozen_merge_refused: true,
            };
        }
        AppendedFragment {
            group: self.push_with_fragment(name, fragment),
            frozen_merge_refused: false,
        }
    }

    pub fn get(&self, id: TypeGroupId) -> Option<&TypeGroup> {
        self.groups.get(id.index())
    }

    pub(crate) fn get_mut(&mut self, id: TypeGroupId) -> Option<&mut TypeGroup> {
        self.groups.get_mut_local(id.index())
    }

    pub fn len(&self) -> usize {
        self.groups.len()
    }

    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &TypeGroup> {
        self.groups.iter()
    }

    #[cfg(test)]
    pub(crate) fn local_groups(&self) -> impl Iterator<Item = &TypeGroup> {
        self.groups.local_iter()
    }

    #[cfg(test)]
    pub(crate) fn local_row_count_for_test(&self) -> usize {
        self.groups.local_len()
    }

    #[cfg(test)]
    pub(crate) fn from_snapshot(groups: Vec<TypeGroup>) -> Result<Self, &'static str> {
        if groups
            .iter()
            .enumerate()
            .any(|(index, group)| group.id.index() != index)
        {
            return Err("snapshot type-group ids are not dense");
        }
        let mut table = Self::default();
        for group in groups {
            table.groups.push_local(group);
        }
        Ok(table)
    }

    pub(crate) fn freeze_as_base(&mut self) -> Result<(), &'static str> {
        self.groups.freeze_as_base()
    }

    pub(crate) fn fork_delta(&self) -> Result<Self, &'static str> {
        Ok(Self {
            groups: self.groups.fork_delta()?,
        })
    }

    #[cfg(test)]
    pub(crate) fn shares_base_storage_with(&self, other: &Self) -> bool {
        self.groups.shares_base_with(&other.groups)
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

    #[test]
    fn occurrence_only_projection_allocates_no_global_census_state() {
        let source = "declare var globalValue: number; interface GlobalType {} declare global { namespace Nested {} }";
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::d_ts()).parse();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

        let mut occurrence_only = SourceDeclarationVisitor::occurrences_only();
        occurrence_only.visit_program(&parsed.program);
        assert!(occurrence_only.global_census.is_none());
        let occurrences = occurrence_only
            .occurrences
            .expect("occurrence-only projection owns occurrence rows");

        let census = source_global_binding_census(
            &parsed.program,
            ModuleBindingContext::for_program(
                &parsed.program,
                crate::binder::namespace::SourceFileKind::DeclarationTs,
            ),
        );
        assert_eq!(occurrences, source_declaration_occurrences(&parsed.program));
        assert!(census.source_nodes_visited > 0);
        assert!(!census.candidates.is_empty());
    }

    #[test]
    fn global_census_excludes_illegal_script_augmentation_but_keeps_external_global() {
        let allocator = Allocator::default();
        let illegal = Parser::new(
            &allocator,
            "declare global { interface InvalidScriptGlobal {} }",
            SourceType::d_ts(),
        )
        .parse();
        assert!(illegal.diagnostics.is_empty(), "{:?}", illegal.diagnostics);
        let illegal = source_global_binding_census(
            &illegal.program,
            ModuleBindingContext::for_program(
                &illegal.program,
                crate::binder::namespace::SourceFileKind::DeclarationTs,
            ),
        );
        assert!(!illegal.candidates.contains_key("InvalidScriptGlobal"));
        assert!(illegal.uncertain_candidates.is_empty());

        let allocator = Allocator::default();
        let legal = Parser::new(
            &allocator,
            "export {}; declare global { interface LegalExternalGlobal {} }",
            SourceType::d_ts(),
        )
        .parse();
        assert!(legal.diagnostics.is_empty(), "{:?}", legal.diagnostics);
        let legal = source_global_binding_census(
            &legal.program,
            ModuleBindingContext::for_program(
                &legal.program,
                crate::binder::namespace::SourceFileKind::DeclarationTs,
            ),
        );
        assert_eq!(
            legal.candidates["LegalExternalGlobal"].slots,
            BTreeSet::from([SourceBindingSlot::Type])
        );
        assert!(legal.uncertain_candidates.is_empty());
    }

    #[test]
    fn uncertain_same_name_global_does_not_contaminate_exact_slots() {
        let allocator = Allocator::default();
        let parsed = Parser::new(
            &allocator,
            "export {}; declare global { interface Shared {} } { global { var Shared: number; } }",
            SourceType::ts(),
        )
        .parse();
        let census = source_global_binding_census(
            &parsed.program,
            ModuleBindingContext::for_program(
                &parsed.program,
                crate::binder::namespace::SourceFileKind::ImplementationTs,
            ),
        );
        assert_eq!(
            census.candidates["Shared"].slots,
            BTreeSet::from([SourceBindingSlot::Type])
        );
        assert_eq!(
            census.uncertain_candidates["Shared"].slots,
            BTreeSet::from([SourceBindingSlot::Value])
        );
    }
}

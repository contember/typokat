//! Owned structural projections of the declaration visitor, without semantic resolution.

use crate::span::Span;
use oxc_ast::ast::{BindingPattern, VariableDeclarationKind};
use oxc_ast::{AstKind, AstType};
use oxc_span::GetSpan;

macro_rules! source_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $name(pub u32);

        impl $name {
            pub fn index(self) -> usize {
                usize::try_from(self.0).expect("source identity fits usize")
            }
        }
    };
}

source_id!(SourceReferenceId);
source_id!(SourceOperationId);
source_id!(SourceRegionId);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookupSpace {
    Value,
    Type,
    QueryValue,
    Export,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteKind {
    Read,
    Write,
    ReadWrite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeRole {
    Root,
    Child,
    ProgramBody,
    Binding,
    Initializer,
    Annotation,
    TypeArguments,
    TypeName,
    QueryName,
    QualifiedRoot,
    MemberName,
    Callee,
    Argument,
    Target,
    Expression,
    Parenthesized,
    Shorthand,
    ComputedKey,
    StaticKey,
    Spread,
    Return,
    Default,
    Body,
    Header,
    ExportLocal,
    ExportedName,
    ImportQualifier,
    JsxRoot,
    JsxMember,
    JsxExpression,
    LoopSource,
    ForInitializer,
    ForTest,
    ForUpdate,
    SwitchDiscriminant,
    CaseTest,
    Decorator,
    Heritage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceCoverage {
    Unavailable,
    /// The Program API supplies no parser diagnostics or semantic scope authentication.
    StructuralTraversal,
    InvalidProjection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoverageHole {
    MissingProgram,
    UnbalancedTraversal,
    InvalidSpan,
    MissingRowsOrEdges,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceOwners {
    pub callable: Option<SourceOperationId>,
    pub class: Option<SourceOperationId>,
    pub namespace: Option<SourceOperationId>,
    pub header: Option<SourceOperationId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceScopeAnchor {
    pub operation: SourceOperationId,
    pub syntax: AstType,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceRegion {
    pub id: SourceRegionId,
    pub parent: Option<SourceRegionId>,
    pub anchor: SourceScopeAnchor,
    pub owners: SourceOwners,
    pub direct_program_body: bool,
    pub exited: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceCallFacts {
    pub explicit_type_arguments: bool,
    pub value_argument_count: usize,
    pub optional: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceVariableFacts {
    pub kind: VariableDeclarationKind,
    pub single_declarator: bool,
    pub single_identifier: Option<Span>,
    pub annotated: bool,
    pub has_initializer: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceDeclarationFacts {
    pub kind: super::declaration::DeclarationKind,
    pub declare_modifier: bool,
    pub expression: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceOperation {
    pub id: SourceOperationId,
    pub span: Span,
    pub syntax: AstType,
    pub parent: Option<SourceOperationId>,
    pub edge: EdgeRole,
    /// Ordinal among generated enter-node children, including names and type syntax.
    pub child_ordinal: u32,
    pub child_count: u32,
    pub region: SourceRegionId,
    pub owners: SourceOwners,
    pub direct_program_body: bool,
    pub name: Option<String>,
    pub call: Option<SourceCallFacts>,
    pub variable: Option<SourceVariableFacts>,
    pub declaration: Option<SourceDeclarationFacts>,
    pub type_only: bool,
    /// Parentheses inherit this edge's operation; other syntax starts a new result context.
    pub result_context: Option<SourceOperationId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceReference {
    pub id: SourceReferenceId,
    pub span: Span,
    pub name: String,
    pub space: LookupSpace,
    pub write: WriteKind,
    pub operation: SourceOperationId,
    pub region: SourceRegionId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceCopyCandidate {
    pub declaration: SourceOperationId,
    pub binding_span: Span,
    pub reference: SourceReferenceId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceBoundaryKind {
    Import,
    Export,
    ImportQualifier,
    QualifiedMember,
    GlobalThisSpelling,
    This,
    Dynamic,
    Jsx,
    Namespace,
    GlobalAugmentation,
    Directive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceBoundary {
    pub operation: SourceOperationId,
    pub kind: SourceBoundaryKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceReferenceWork {
    pub nodes_entered: u64,
    pub nodes_left: u64,
    pub edges_processed: u64,
    pub reference_rows: u64,
    pub rows_validated: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceReferenceManifest {
    coverage: ReferenceCoverage,
    holes: Box<[CoverageHole]>,
    references: Box<[SourceReference]>,
    operations: Box<[SourceOperation]>,
    regions: Box<[SourceRegion]>,
    copy_candidates: Box<[SourceCopyCandidate]>,
    boundaries: Box<[SourceBoundary]>,
    work: SourceReferenceWork,
}

impl SourceReferenceManifest {
    pub(crate) fn unavailable() -> Self {
        Self {
            coverage: ReferenceCoverage::Unavailable,
            holes: Box::new([]),
            references: Box::new([]),
            operations: Box::new([]),
            regions: Box::new([]),
            copy_candidates: Box::new([]),
            boundaries: Box::new([]),
            work: SourceReferenceWork::default(),
        }
    }

    pub fn coverage(&self) -> ReferenceCoverage {
        self.coverage
    }
    pub fn holes(&self) -> &[CoverageHole] {
        &self.holes
    }
    pub fn references(&self) -> &[SourceReference] {
        &self.references
    }
    pub fn operations(&self) -> &[SourceOperation] {
        &self.operations
    }
    pub fn regions(&self) -> &[SourceRegion] {
        &self.regions
    }
    pub fn copy_candidates(&self) -> &[SourceCopyCandidate] {
        &self.copy_candidates
    }
    pub fn boundaries(&self) -> &[SourceBoundary] {
        &self.boundaries
    }

    #[cfg(any(test, feature = "test-utils", feature = "measurement-counters"))]
    pub fn work_for_test(&self) -> SourceReferenceWork {
        self.work
    }
}

struct ReferenceFrame<'ast> {
    kind: AstKind<'ast>,
    operation: SourceOperationId,
    region: SourceRegionId,
    opens_region: bool,
    children: u32,
    space: LookupSpace,
    write: WriteKind,
    copy: Option<(SourceOperationId, Span)>,
    arrow_expression_body: bool,
    external_export: bool,
    suppress_reference: bool,
}

#[derive(Default)]
pub(super) struct SourceReferenceBuilder<'ast> {
    stack: Vec<ReferenceFrame<'ast>>,
    operations: Vec<SourceOperation>,
    regions: Vec<SourceRegion>,
    references: Vec<SourceReference>,
    copies: Vec<SourceCopyCandidate>,
    boundaries: Vec<SourceBoundary>,
    holes: Vec<CoverageHole>,
    work: SourceReferenceWork,
}

fn dense(length: usize) -> u32 {
    u32::try_from(length).expect("source fact count fits u32")
}

impl<'ast> SourceReferenceBuilder<'ast> {
    pub(super) fn enter(&mut self, kind: AstKind<'ast>) {
        self.work.nodes_entered += 1;
        let id = SourceOperationId(dense(self.operations.len()));
        let span = Span::from_oxc(kind.span());
        let parent = self.stack.last();
        let edge = parent.map_or(EdgeRole::Root, |parent| edge_role(parent, kind));
        let parent_id = parent.map(|parent| parent.operation);
        let child_ordinal = parent.map_or(0, |parent| parent.children);
        let parent_operation = parent_id.map(|id| &self.operations[id.index()]);
        let direct_program_body = parent
            .is_some_and(|parent| matches!(parent.kind, AstKind::Program(_)))
            && edge == EdgeRole::ProgramBody;
        let mut owners = parent_operation.map_or(SourceOwners::default(), |parent| parent.owners);
        match kind {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                owners.callable = Some(id);
                owners.header = Some(id);
            }
            AstKind::Class(_) => {
                owners.class = Some(id);
                owners.header = Some(id);
            }
            AstKind::TSModuleDeclaration(_) | AstKind::TSGlobalDeclaration(_) => {
                owners.namespace = Some(id);
            }
            AstKind::TSTypeAliasDeclaration(_)
            | AstKind::TSInterfaceDeclaration(_)
            | AstKind::TSTypeParameterDeclaration(_)
            | AstKind::TSFunctionType(_)
            | AstKind::TSConstructorType(_)
            | AstKind::TSMappedType(_)
            | AstKind::TSInferType(_)
            | AstKind::TSConditionalType(_)
            | AstKind::TSCallSignatureDeclaration(_)
            | AstKind::TSMethodSignature(_)
            | AstKind::TSConstructSignatureDeclaration(_) => {
                owners.header = Some(id);
            }
            AstKind::FunctionBody(_) | AstKind::ClassBody(_) => {
                owners.header = None;
            }
            _ => {}
        }
        let opens_region = parent.is_none()
            || is_region(kind.ty())
            || matches!(
                edge,
                EdgeRole::LoopSource | EdgeRole::SwitchDiscriminant | EdgeRole::CaseTest
            );
        let region = if opens_region {
            SourceRegionId(dense(self.regions.len()))
        } else {
            parent
                .expect("non-root region inherits syntax anchor")
                .region
        };
        let space = match edge {
            EdgeRole::QueryName => LookupSpace::QueryValue,
            EdgeRole::TypeName | EdgeRole::Annotation | EdgeRole::TypeArguments => {
                LookupSpace::Type
            }
            EdgeRole::ExportLocal => LookupSpace::Export,
            EdgeRole::ComputedKey | EdgeRole::Initializer | EdgeRole::Default => LookupSpace::Value,
            _ => parent.map_or(LookupSpace::Value, |parent| parent.space),
        };
        let write = match edge {
            EdgeRole::Target => match parent.map(|parent| parent.kind) {
                Some(AstKind::UpdateExpression(_)) => WriteKind::ReadWrite,
                Some(AstKind::AssignmentExpression(expression))
                    if !expression.operator.is_assign() =>
                {
                    WriteKind::ReadWrite
                }
                _ => WriteKind::Write,
            },
            EdgeRole::Parenthesized => parent.map_or(WriteKind::Read, |parent| parent.write),
            EdgeRole::Expression
                if parent.is_some_and(|parent| {
                    matches!(
                        parent.kind,
                        AstKind::TSAsExpression(_)
                            | AstKind::TSTypeAssertion(_)
                            | AstKind::TSNonNullExpression(_)
                            | AstKind::TSSatisfiesExpression(_)
                    )
                }) =>
            {
                parent.expect("assertion parent").write
            }
            _ => WriteKind::Read,
        };
        let variable = match kind {
            AstKind::VariableDeclarator(declaration) => Some(SourceVariableFacts {
                kind: declaration.kind,
                single_declarator: parent.is_some_and(|parent| matches!(parent.kind, AstKind::VariableDeclaration(declaration) if declaration.declarations.len() == 1)),
                single_identifier: match &declaration.id {
                    BindingPattern::BindingIdentifier(identifier) => Some(Span::from_oxc(identifier.span)),
                    BindingPattern::ObjectPattern(_) | BindingPattern::ArrayPattern(_) | BindingPattern::AssignmentPattern(_) => None,
                },
                annotated: declaration.type_annotation.is_some(),
                has_initializer: declaration.init.is_some(),
            }),
            _ => None,
        };
        let copy = if edge == EdgeRole::Initializer {
            parent_operation.and_then(|parent| {
                let variable = parent.variable?;
                let declaration = parent.parent.map(|id| &self.operations[id.index()])?;
                (declaration.direct_program_body
                    && variable.kind == VariableDeclarationKind::Const
                    && variable.single_declarator
                    && !variable.annotated)
                    .then_some(variable.single_identifier)
                    .flatten()
                    .map(|binding| (parent.id, binding))
            })
        } else if edge == EdgeRole::Parenthesized {
            parent.and_then(|parent| parent.copy)
        } else {
            None
        };
        let result_context = if edge == EdgeRole::Parenthesized {
            parent_operation.and_then(|parent| parent.result_context)
        } else {
            parent_id
        };
        let arrow_expression_body = match kind {
            AstKind::FunctionBody(_) => parent.is_some_and(|parent| matches!(parent.kind, AstKind::ArrowFunctionExpression(arrow) if arrow.expression)),
            AstKind::ExpressionStatement(_) => parent.is_some_and(|parent| parent.arrow_expression_body),
            _ => false,
        };
        let external_export = match kind {
            AstKind::ExportNamedDeclaration(export) => export.source.is_some(),
            _ => parent.is_some_and(|parent| parent.external_export),
        };
        let suppress_reference = matches!(edge, EdgeRole::ImportQualifier | EdgeRole::ExportedName)
            || (edge == EdgeRole::ExportLocal && external_export)
            || parent.is_some_and(|parent| parent.suppress_reference);
        if span.start > span.end
            || parent_operation
                .is_some_and(|parent| span.start < parent.span.start || span.end > parent.span.end)
        {
            self.hole(CoverageHole::InvalidSpan);
        }
        if let Some(parent) = self.stack.last_mut() {
            parent.children += 1;
            self.work.edges_processed += 1;
        } else if !matches!(kind, AstKind::Program(_)) || !self.operations.is_empty() {
            self.hole(CoverageHole::MissingProgram);
        }
        if opens_region {
            self.regions.push(SourceRegion {
                id: region,
                parent: self.stack.last().map(|parent| parent.region),
                anchor: SourceScopeAnchor {
                    operation: id,
                    syntax: kind.ty(),
                    span,
                },
                owners,
                direct_program_body,
                exited: false,
            });
        }
        let call = match kind {
            AstKind::CallExpression(call) => Some(SourceCallFacts {
                explicit_type_arguments: call.type_arguments.is_some(),
                value_argument_count: call.arguments.len(),
                optional: call.optional,
            }),
            AstKind::NewExpression(call) => Some(SourceCallFacts {
                explicit_type_arguments: call.type_arguments.is_some(),
                value_argument_count: call.arguments.len(),
                optional: false,
            }),
            _ => None,
        };
        let type_only = match kind {
            AstKind::ExportNamedDeclaration(node) => node.export_kind.is_type(),
            AstKind::ExportSpecifier(node) => node.export_kind.is_type(),
            AstKind::ImportDeclaration(node) => node.import_kind.is_type(),
            AstKind::ImportSpecifier(node) => node.import_kind.is_type(),
            AstKind::ExportAllDeclaration(node) => node.export_kind.is_type(),
            _ => false,
        };
        self.operations.push(SourceOperation {
            id,
            span,
            syntax: kind.ty(),
            parent: parent_id,
            edge,
            child_ordinal,
            child_count: 0,
            region,
            owners,
            direct_program_body,
            name: node_name(kind),
            call,
            variable,
            declaration: declaration_facts(kind),
            type_only,
            result_context,
        });
        self.stack.push(ReferenceFrame {
            kind,
            operation: id,
            region,
            opens_region,
            children: 0,
            space,
            write,
            copy,
            arrow_expression_body,
            external_export,
            suppress_reference,
        });
        if let AstKind::IdentifierReference(identifier) = kind {
            if !suppress_reference {
                self.reference(identifier.name.as_str(), span);
            }
        }
        if let AstKind::IdentifierName(identifier) = kind {
            if edge == EdgeRole::ExportLocal && !suppress_reference {
                self.reference(identifier.name.as_str(), span);
            }
        }
        if let Some(boundary) = boundary_kind(kind, edge) {
            self.boundaries.push(SourceBoundary {
                operation: id,
                kind: boundary,
            });
        }
    }

    fn reference(&mut self, name: &str, span: Span) {
        let frame = self.stack.last().expect("reference node entered");
        let id = SourceReferenceId(dense(self.references.len()));
        self.references.push(SourceReference {
            id,
            span,
            name: name.to_owned(),
            space: frame.space,
            write: frame.write,
            operation: frame.operation,
            region: frame.region,
        });
        self.work.reference_rows += 1;
        if let Some((declaration, binding_span)) = frame.copy {
            self.copies.push(SourceCopyCandidate {
                declaration,
                binding_span,
                reference: id,
            });
        }
    }

    pub(super) fn leave(&mut self, kind: AstKind<'ast>) {
        self.work.nodes_left += 1;
        let Some(frame) = self.stack.pop() else {
            self.hole(CoverageHole::UnbalancedTraversal);
            return;
        };
        if frame.kind.ty() != kind.ty() || frame.kind.span() != kind.span() {
            self.hole(CoverageHole::UnbalancedTraversal);
        }
        self.operations[frame.operation.index()].child_count = frame.children;
        if frame.opens_region {
            self.regions[frame.region.index()].exited = true;
        }
    }

    fn hole(&mut self, hole: CoverageHole) {
        if !self.holes.contains(&hole) {
            self.holes.push(hole);
        }
    }

    pub(super) fn finish(mut self) -> SourceReferenceManifest {
        if self
            .operations
            .first()
            .is_none_or(|operation| operation.syntax != AstType::Program)
        {
            self.hole(CoverageHole::MissingProgram);
        }
        if !self.stack.is_empty() || self.work.nodes_entered != self.work.nodes_left {
            self.hole(CoverageHole::UnbalancedTraversal);
        }
        let rows = u64::try_from(self.operations.len()).expect("operation count fits u64");
        if rows != self.work.nodes_entered
            || self.work.edges_processed + 1 != rows
            || self.work.reference_rows
                != u64::try_from(self.references.len()).expect("reference count fits u64")
        {
            self.hole(CoverageHole::MissingRowsOrEdges);
        }
        if !self.validate_rows() {
            self.hole(CoverageHole::MissingRowsOrEdges);
        }
        SourceReferenceManifest {
            coverage: if self.holes.is_empty() {
                ReferenceCoverage::StructuralTraversal
            } else {
                ReferenceCoverage::InvalidProjection
            },
            holes: self.holes.into_boxed_slice(),
            references: self.references.into_boxed_slice(),
            operations: self.operations.into_boxed_slice(),
            regions: self.regions.into_boxed_slice(),
            copy_candidates: self.copies.into_boxed_slice(),
            boundaries: self.boundaries.into_boxed_slice(),
            work: self.work,
        }
    }

    fn validate_rows(&mut self) -> bool {
        let mut valid = true;
        let mut children = vec![0_u32; self.operations.len()];
        for (index, operation) in self.operations.iter().enumerate() {
            self.work.rows_validated += 1;
            valid &= operation.id.index() == index && operation.region.index() < self.regions.len();
            if let Some(parent) = operation.parent {
                if parent.index() < index {
                    valid &= operation.child_ordinal == children[parent.index()];
                    children[parent.index()] += 1;
                } else {
                    valid = false;
                }
            } else {
                valid &= index == 0;
            }
        }
        for (operation, children) in self.operations.iter().zip(children) {
            self.work.rows_validated += 1;
            valid &= operation.child_count == children;
        }
        for (index, reference) in self.references.iter().enumerate() {
            self.work.rows_validated += 1;
            valid &= reference.id.index() == index
                && self
                    .operations
                    .get(reference.operation.index())
                    .is_some_and(|operation| {
                        operation.span == reference.span
                            && operation.region == reference.region
                            && operation.name.as_deref() == Some(reference.name.as_str())
                    });
        }
        for (index, region) in self.regions.iter().enumerate() {
            self.work.rows_validated += 1;
            valid &= region.id.index() == index
                && region.exited
                && region.parent.is_none_or(|parent| parent.index() < index)
                && self
                    .operations
                    .get(region.anchor.operation.index())
                    .is_some_and(|operation| {
                        operation.region == region.id
                            && operation.span == region.anchor.span
                            && operation.syntax == region.anchor.syntax
                    });
        }
        for candidate in &self.copies {
            self.work.rows_validated += 1;
            valid &= candidate.reference.index() < self.references.len()
                && self
                    .operations
                    .get(candidate.declaration.index())
                    .is_some_and(|operation| {
                        operation.variable.is_some_and(|variable| {
                            variable.single_identifier == Some(candidate.binding_span)
                        })
                    });
        }
        for boundary in &self.boundaries {
            self.work.rows_validated += 1;
            valid &= boundary.operation.index() < self.operations.len();
        }
        valid
    }

    #[cfg(test)]
    pub(super) fn remove_last_edge_for_test(&mut self) {
        if let Some(operation) = self.operations.last_mut() {
            operation.parent = None;
        }
    }

    #[cfg(test)]
    pub(super) fn remove_last_reference_for_test(&mut self) {
        self.references.pop();
    }
}

fn declaration_facts(kind: AstKind<'_>) -> Option<SourceDeclarationFacts> {
    use super::declaration::DeclarationKind;
    let (kind, declare_modifier, expression) = match kind {
        AstKind::VariableDeclaration(node) => (DeclarationKind::Variable, node.declare, false),
        AstKind::VariableDeclarator(_) => (DeclarationKind::Variable, false, false),
        AstKind::Function(node) => (
            DeclarationKind::Function,
            node.declare,
            !node.is_declaration(),
        ),
        AstKind::Class(node) => (
            DeclarationKind::Class,
            node.declare,
            node.r#type == oxc_ast::ast::ClassType::ClassExpression,
        ),
        AstKind::TSTypeAliasDeclaration(node) => (DeclarationKind::TypeAlias, node.declare, false),
        AstKind::TSInterfaceDeclaration(node) => (DeclarationKind::Interface, node.declare, false),
        AstKind::TSEnumDeclaration(node) => (DeclarationKind::Enum, node.declare, false),
        AstKind::TSModuleDeclaration(node) => (DeclarationKind::Namespace, node.declare, false),
        AstKind::TSGlobalDeclaration(node) => (DeclarationKind::Global, node.declare, false),
        AstKind::FormalParameter(_) | AstKind::FormalParameterRest(_) => {
            (DeclarationKind::Parameter, false, false)
        }
        AstKind::CatchParameter(_) => (DeclarationKind::CatchParameter, false, false),
        AstKind::ImportDeclaration(_) => (DeclarationKind::Import, false, false),
        AstKind::TSImportEqualsDeclaration(_) => (DeclarationKind::ImportEquals, false, false),
        AstKind::TSNamespaceExportDeclaration(_) => {
            (DeclarationKind::NamespaceExport, false, false)
        }
        _ => return None,
    };
    Some(SourceDeclarationFacts {
        kind,
        declare_modifier,
        expression,
    })
}

fn node_name(kind: AstKind<'_>) -> Option<String> {
    match kind {
        AstKind::IdentifierReference(node) => Some(node.name.to_string()),
        AstKind::IdentifierName(node) => Some(node.name.to_string()),
        AstKind::BindingIdentifier(node) => Some(node.name.to_string()),
        AstKind::LabelIdentifier(node) => Some(node.name.to_string()),
        AstKind::JSXIdentifier(node) => Some(node.name.to_string()),
        AstKind::PrivateIdentifier(node) => Some(node.name.to_string()),
        AstKind::StringLiteral(node) => Some(node.value.to_string()),
        AstKind::Directive(node) => Some(node.directive.to_string()),
        _ => None,
    }
}

fn is_region(syntax: AstType) -> bool {
    matches!(
        syntax,
        AstType::Program
            | AstType::Function
            | AstType::ArrowFunctionExpression
            | AstType::FunctionBody
            | AstType::Class
            | AstType::ClassBody
            | AstType::StaticBlock
            | AstType::BlockStatement
            | AstType::TSModuleDeclaration
            | AstType::TSModuleBlock
            | AstType::TSGlobalDeclaration
            | AstType::FormalParameters
            | AstType::TSTypeParameterDeclaration
            | AstType::TSTypeAliasDeclaration
            | AstType::TSInterfaceDeclaration
            | AstType::TSFunctionType
            | AstType::TSConstructorType
            | AstType::TSMappedType
            | AstType::TSInferType
            | AstType::CatchClause
            | AstType::TSEnumBody
            | AstType::TSConditionalType
            | AstType::TSCallSignatureDeclaration
            | AstType::TSMethodSignature
            | AstType::TSConstructSignatureDeclaration
            | AstType::ForStatement
            | AstType::ForInStatement
            | AstType::ForOfStatement
            | AstType::SwitchStatement
            | AstType::SwitchCase
            | AstType::WithStatement
    )
}

fn boundary_kind(kind: AstKind<'_>, edge: EdgeRole) -> Option<SourceBoundaryKind> {
    if edge == EdgeRole::ImportQualifier {
        return Some(SourceBoundaryKind::ImportQualifier);
    }
    match kind {
        AstKind::ImportDeclaration(_)
        | AstKind::TSImportType(_)
        | AstKind::TSImportEqualsDeclaration(_) => Some(SourceBoundaryKind::Import),
        AstKind::ExportNamedDeclaration(_)
        | AstKind::ExportDefaultDeclaration(_)
        | AstKind::ExportAllDeclaration(_)
        | AstKind::TSExportAssignment(_)
        | AstKind::TSNamespaceExportDeclaration(_) => Some(SourceBoundaryKind::Export),
        AstKind::TSQualifiedName(_)
        | AstKind::StaticMemberExpression(_)
        | AstKind::PrivateFieldExpression(_) => Some(SourceBoundaryKind::QualifiedMember),
        AstKind::IdentifierReference(identifier) if identifier.name == "globalThis" => {
            Some(SourceBoundaryKind::GlobalThisSpelling)
        }
        AstKind::IdentifierReference(identifier) if identifier.name == "eval" => {
            Some(SourceBoundaryKind::Dynamic)
        }
        AstKind::ThisExpression(_) => Some(SourceBoundaryKind::This),
        AstKind::ComputedMemberExpression(_)
        | AstKind::WithStatement(_)
        | AstKind::ImportExpression(_)
        | AstKind::V8IntrinsicExpression(_) => Some(SourceBoundaryKind::Dynamic),
        AstKind::JSXElement(_)
        | AstKind::JSXFragment(_)
        | AstKind::JSXIdentifier(_)
        | AstKind::JSXMemberExpression(_) => Some(SourceBoundaryKind::Jsx),
        AstKind::TSModuleDeclaration(_) => Some(SourceBoundaryKind::Namespace),
        AstKind::TSGlobalDeclaration(_) => Some(SourceBoundaryKind::GlobalAugmentation),
        AstKind::Directive(_) => Some(SourceBoundaryKind::Directive),
        _ => None,
    }
}

fn edge_role(parent: &ReferenceFrame<'_>, child: AstKind<'_>) -> EdgeRole {
    let index = parent.children;
    let span = child.span();
    match parent.kind {
        AstKind::Program(_) => {
            if matches!(child, AstKind::Directive(_) | AstKind::Hashbang(_)) {
                EdgeRole::Child
            } else {
                EdgeRole::ProgramBody
            }
        }
        AstKind::ParenthesizedExpression(_) => EdgeRole::Parenthesized,
        AstKind::VariableDeclarator(declaration) => {
            if index == 0 {
                EdgeRole::Binding
            } else if declaration
                .type_annotation
                .as_ref()
                .is_some_and(|annotation| annotation.span == span)
            {
                EdgeRole::Annotation
            } else {
                EdgeRole::Initializer
            }
        }
        AstKind::CallExpression(_) | AstKind::NewExpression(_) => {
            if index == 0 {
                EdgeRole::Callee
            } else if matches!(child, AstKind::TSTypeParameterInstantiation(_)) {
                EdgeRole::TypeArguments
            } else {
                EdgeRole::Argument
            }
        }
        AstKind::TSTypeQuery(_) => {
            if index == 0 {
                EdgeRole::QueryName
            } else {
                EdgeRole::TypeArguments
            }
        }
        AstKind::TSTypeReference(_) => {
            if index == 0 {
                EdgeRole::TypeName
            } else {
                EdgeRole::TypeArguments
            }
        }
        AstKind::TSQualifiedName(_) => {
            if index == 0 {
                EdgeRole::QualifiedRoot
            } else {
                EdgeRole::MemberName
            }
        }
        AstKind::TSImportType(node) => {
            if node
                .qualifier
                .as_ref()
                .is_some_and(|qualifier| qualifier.span() == span)
            {
                EdgeRole::ImportQualifier
            } else if matches!(child, AstKind::TSTypeParameterInstantiation(_)) {
                EdgeRole::TypeArguments
            } else {
                EdgeRole::Child
            }
        }
        AstKind::TSImportTypeQualifiedName(_) => EdgeRole::ImportQualifier,
        AstKind::TSImportEqualsDeclaration(_) => {
            if index == 0 {
                EdgeRole::Binding
            } else {
                EdgeRole::ImportQualifier
            }
        }
        AstKind::ExportSpecifier(_) => {
            if index == 0 {
                EdgeRole::ExportLocal
            } else {
                EdgeRole::ExportedName
            }
        }
        AstKind::AssignmentExpression(_) => {
            if index == 0 {
                EdgeRole::Target
            } else {
                EdgeRole::Expression
            }
        }
        AstKind::UpdateExpression(_) => EdgeRole::Target,
        AstKind::ForStatement(statement) => {
            if statement
                .init
                .as_ref()
                .is_some_and(|init| init.span() == span)
            {
                EdgeRole::ForInitializer
            } else if statement
                .test
                .as_ref()
                .is_some_and(|test| test.span() == span)
            {
                EdgeRole::ForTest
            } else if statement
                .update
                .as_ref()
                .is_some_and(|update| update.span() == span)
            {
                EdgeRole::ForUpdate
            } else {
                EdgeRole::Body
            }
        }
        AstKind::ForInStatement(_) | AstKind::ForOfStatement(_) => match index {
            0 => EdgeRole::Target,
            1 => EdgeRole::LoopSource,
            _ => EdgeRole::Body,
        },
        AstKind::ObjectAssignmentTarget(_)
        | AstKind::ArrayAssignmentTarget(_)
        | AstKind::AssignmentTargetRest(_) => EdgeRole::Target,
        AstKind::AssignmentTargetPropertyProperty(node) => {
            if index == 0 {
                if node.computed {
                    EdgeRole::ComputedKey
                } else {
                    EdgeRole::StaticKey
                }
            } else {
                EdgeRole::Target
            }
        }
        AstKind::AssignmentTargetPropertyIdentifier(_)
        | AstKind::AssignmentTargetWithDefault(_) => {
            if index == 0 {
                EdgeRole::Target
            } else {
                EdgeRole::Default
            }
        }
        AstKind::AssignmentPattern(_) => {
            if index == 0 {
                EdgeRole::Binding
            } else {
                EdgeRole::Default
            }
        }
        AstKind::FormalParameter(parameter) => {
            if parameter
                .initializer
                .as_ref()
                .is_some_and(|initializer| initializer.span() == span)
            {
                EdgeRole::Default
            } else if matches!(child, AstKind::TSTypeAnnotation(_)) {
                EdgeRole::Annotation
            } else {
                EdgeRole::Binding
            }
        }
        AstKind::ObjectProperty(property) => {
            if index == 0 {
                if property.computed {
                    EdgeRole::ComputedKey
                } else {
                    EdgeRole::StaticKey
                }
            } else if property.shorthand {
                EdgeRole::Shorthand
            } else {
                EdgeRole::Initializer
            }
        }
        AstKind::BindingProperty(property) => {
            if index == 0 {
                if property.computed {
                    EdgeRole::ComputedKey
                } else {
                    EdgeRole::StaticKey
                }
            } else {
                EdgeRole::Binding
            }
        }
        AstKind::PropertyDefinition(property) => {
            if property.key.span() == span {
                if property.computed {
                    EdgeRole::ComputedKey
                } else {
                    EdgeRole::StaticKey
                }
            } else if matches!(child, AstKind::TSTypeAnnotation(_)) {
                EdgeRole::Annotation
            } else if property
                .value
                .as_ref()
                .is_some_and(|value| value.span() == span)
            {
                EdgeRole::Initializer
            } else {
                EdgeRole::Child
            }
        }
        AstKind::AccessorProperty(property) => {
            if property.key.span() == span {
                if property.computed {
                    EdgeRole::ComputedKey
                } else {
                    EdgeRole::StaticKey
                }
            } else if matches!(child, AstKind::TSTypeAnnotation(_)) {
                EdgeRole::Annotation
            } else if property
                .value
                .as_ref()
                .is_some_and(|value| value.span() == span)
            {
                EdgeRole::Initializer
            } else {
                EdgeRole::Child
            }
        }
        AstKind::MethodDefinition(method) => {
            if method.key.span() == span {
                if method.computed {
                    EdgeRole::ComputedKey
                } else {
                    EdgeRole::StaticKey
                }
            } else if matches!(child, AstKind::Decorator(_)) {
                EdgeRole::Decorator
            } else {
                EdgeRole::Child
            }
        }
        AstKind::TSPropertySignature(property) => {
            if property.key.span() == span {
                if property.computed {
                    EdgeRole::ComputedKey
                } else {
                    EdgeRole::StaticKey
                }
            } else {
                EdgeRole::Annotation
            }
        }
        AstKind::TSMethodSignature(method) => {
            if method.key.span() == span {
                if method.computed {
                    EdgeRole::ComputedKey
                } else {
                    EdgeRole::StaticKey
                }
            } else if matches!(child, AstKind::TSTypeAnnotation(_)) {
                EdgeRole::Annotation
            } else {
                EdgeRole::Header
            }
        }
        AstKind::SpreadElement(_) | AstKind::JSXSpreadAttribute(_) | AstKind::JSXSpreadChild(_) => {
            EdgeRole::Spread
        }
        AstKind::ReturnStatement(_) => EdgeRole::Return,
        AstKind::ExpressionStatement(_) => {
            if parent.arrow_expression_body {
                EdgeRole::Return
            } else {
                EdgeRole::Expression
            }
        }
        AstKind::TSAsExpression(_) | AstKind::TSSatisfiesExpression(_) => {
            if index == 0 {
                EdgeRole::Expression
            } else {
                EdgeRole::Annotation
            }
        }
        AstKind::TSTypeAssertion(_) => {
            if index == 0 {
                EdgeRole::Annotation
            } else {
                EdgeRole::Expression
            }
        }
        AstKind::TSNonNullExpression(_) => EdgeRole::Expression,
        AstKind::TSInstantiationExpression(_) => {
            if index == 0 {
                EdgeRole::Expression
            } else {
                EdgeRole::TypeArguments
            }
        }
        AstKind::TSInterfaceHeritage(_) | AstKind::TSClassImplements(_) => {
            if index == 0 {
                EdgeRole::TypeName
            } else {
                EdgeRole::TypeArguments
            }
        }
        AstKind::Class(class) => {
            if class
                .super_class
                .as_ref()
                .is_some_and(|heritage| heritage.span() == span)
            {
                EdgeRole::Heritage
            } else if matches!(child, AstKind::Decorator(_)) {
                EdgeRole::Decorator
            } else {
                EdgeRole::Child
            }
        }
        AstKind::Decorator(_) => EdgeRole::Expression,
        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
            if matches!(child, AstKind::FunctionBody(_)) {
                EdgeRole::Body
            } else if matches!(child, AstKind::TSTypeAnnotation(_)) {
                EdgeRole::Annotation
            } else {
                EdgeRole::Header
            }
        }
        AstKind::StaticMemberExpression(_) | AstKind::PrivateFieldExpression(_) => {
            if index == 0 {
                EdgeRole::Expression
            } else {
                EdgeRole::MemberName
            }
        }
        AstKind::ComputedMemberExpression(_) => {
            if index == 0 {
                EdgeRole::Expression
            } else {
                EdgeRole::ComputedKey
            }
        }
        AstKind::JSXOpeningElement(_) | AstKind::JSXClosingElement(_) => {
            if index == 0 {
                EdgeRole::JsxRoot
            } else {
                EdgeRole::Child
            }
        }
        AstKind::JSXMemberExpression(_) => {
            if index == 0 {
                EdgeRole::JsxRoot
            } else {
                EdgeRole::JsxMember
            }
        }
        AstKind::JSXExpressionContainer(_) => EdgeRole::JsxExpression,
        AstKind::SwitchStatement(_) => {
            if index == 0 {
                EdgeRole::SwitchDiscriminant
            } else {
                EdgeRole::Body
            }
        }
        AstKind::SwitchCase(case) => {
            if index == 0 && case.test.is_some() {
                EdgeRole::CaseTest
            } else {
                EdgeRole::Body
            }
        }
        _ => EdgeRole::Child,
    }
}

//! Compile-time OXC surface classifiers (sprint 2026-07-10, WU1).
//!
//! Inert wiring. Each exhaustive `match` maps every `oxc 0.137.0` AST variant of a
//! dispatch enum to a stable, oxc-independent surface-family name. There is no
//! `_` arm: an OXC upgrade that adds a variant fails to compile (the drift
//! tripwire). `tests/surface.rs` cross-checks these families against the
//! checked-in `tests/surface/inventory.toml`. WU2/WU3+ consume the classifiers;
//! this stage changes no checker behavior.

use oxc_ast::ast::{
    match_assignment_target, match_expression, match_ts_type, BindingPattern, ClassElement,
    Declaration, Expression, ForStatementInit, ForStatementLeft, ModuleDeclaration,
    ObjectPropertyKind, Statement, TSLiteral, TSSignature, TSTupleElement, TSType, TSTypeName,
};

/// Generates an exhaustive classifier `fn` plus the `const` list of every surface
/// family it can emit (the validator's source of truth). A new OXC variant breaks
/// the `match`; the const then updates from the same table.
macro_rules! surface_classifier {
    ($fn:ident, $consts:ident, $ty:ty, $($variant:pat => $surface:literal,)+) => {
        pub fn $fn(node: &$ty) -> &'static str {
            match node {
                $($variant => $surface,)+
            }
        }
        pub const $consts: &[&str] = &[$($surface,)+];
    };
}

surface_classifier!(
    expression_surface,
    EXPRESSION_SURFACES,
    Expression<'_>,
    Expression::BooleanLiteral(_) => "literal",
    Expression::NullLiteral(_) => "literal",
    Expression::NumericLiteral(_) => "literal",
    Expression::StringLiteral(_) => "literal",
    Expression::BigIntLiteral(_) => "bigint-literal",
    Expression::RegExpLiteral(_) => "regexp-literal",
    Expression::TemplateLiteral(_) => "template-literal",
    Expression::Identifier(_) => "identifier-reference",
    Expression::MetaProperty(_) => "meta-property",
    Expression::Super(_) => "super",
    Expression::ArrayExpression(_) => "array-literal",
    Expression::ArrowFunctionExpression(_) => "arrow-function",
    Expression::AssignmentExpression(_) => "assignment-expression",
    Expression::AwaitExpression(_) => "await-expression",
    Expression::BinaryExpression(_) => "binary-expression",
    Expression::CallExpression(_) => "call-expression",
    Expression::ChainExpression(_) => "optional-chain",
    Expression::ClassExpression(_) => "class-expression",
    Expression::ConditionalExpression(_) => "conditional-expression",
    Expression::FunctionExpression(_) => "function-expression",
    Expression::ImportExpression(_) => "import-expression",
    Expression::LogicalExpression(_) => "logical-expression",
    Expression::NewExpression(_) => "new-expression",
    Expression::ObjectExpression(_) => "object-literal",
    Expression::ParenthesizedExpression(_) => "parenthesized-expression",
    Expression::SequenceExpression(_) => "sequence-expression",
    Expression::TaggedTemplateExpression(_) => "tagged-template",
    Expression::ThisExpression(_) => "this-expression",
    Expression::UnaryExpression(_) => "unary-expression",
    Expression::UpdateExpression(_) => "update-expression",
    Expression::YieldExpression(_) => "yield-expression",
    Expression::PrivateInExpression(_) => "private-in-expression",
    Expression::JSXElement(_) => "jsx",
    Expression::JSXFragment(_) => "jsx",
    Expression::TSAsExpression(_) => "as-assertion",
    Expression::TSSatisfiesExpression(_) => "satisfies-expression",
    Expression::TSTypeAssertion(_) => "type-assertion",
    Expression::TSNonNullExpression(_) => "non-null-assertion",
    Expression::TSInstantiationExpression(_) => "instantiation-expression",
    Expression::V8IntrinsicExpression(_) => "v8-intrinsic",
    Expression::ComputedMemberExpression(_) => "element-access",
    Expression::StaticMemberExpression(_) => "member-access",
    Expression::PrivateFieldExpression(_) => "private-field-access",
);

surface_classifier!(
    statement_surface,
    STATEMENT_SURFACES,
    Statement<'_>,
    Statement::BlockStatement(_) => "block-statement",
    Statement::BreakStatement(_) => "break-statement",
    Statement::ContinueStatement(_) => "continue-statement",
    Statement::DebuggerStatement(_) => "debugger-statement",
    Statement::DoWhileStatement(_) => "do-while-statement",
    Statement::EmptyStatement(_) => "empty-statement",
    Statement::ExpressionStatement(_) => "expression-statement",
    Statement::ForInStatement(_) => "for-in-statement",
    Statement::ForOfStatement(_) => "for-of-statement",
    Statement::ForStatement(_) => "for-statement",
    Statement::IfStatement(_) => "if-statement",
    Statement::LabeledStatement(_) => "labeled-statement",
    Statement::ReturnStatement(_) => "return-statement",
    Statement::SwitchStatement(_) => "switch-statement",
    Statement::ThrowStatement(_) => "throw-statement",
    Statement::TryStatement(_) => "try-statement",
    Statement::WhileStatement(_) => "while-statement",
    Statement::WithStatement(_) => "with-statement",
    Statement::VariableDeclaration(_) => "variable-declaration",
    Statement::FunctionDeclaration(_) => "function-declaration",
    Statement::ClassDeclaration(_) => "class-declaration",
    Statement::TSTypeAliasDeclaration(_) => "type-alias",
    Statement::TSInterfaceDeclaration(_) => "interface-declaration",
    Statement::TSEnumDeclaration(_) => "enum-declaration",
    Statement::TSModuleDeclaration(_) => "module-declaration",
    Statement::TSGlobalDeclaration(_) => "global-declaration",
    Statement::TSImportEqualsDeclaration(_) => "import-equals",
    Statement::ImportDeclaration(_) => "import-declaration",
    Statement::ExportAllDeclaration(_) => "export-all",
    Statement::ExportDefaultDeclaration(_) => "export-default",
    Statement::ExportNamedDeclaration(_) => "export-named",
    Statement::TSExportAssignment(_) => "export-assignment",
    Statement::TSNamespaceExportDeclaration(_) => "namespace-export",
);

surface_classifier!(
    declaration_surface,
    DECLARATION_SURFACES,
    Declaration<'_>,
    Declaration::VariableDeclaration(_) => "variable-declaration",
    Declaration::FunctionDeclaration(_) => "function-declaration",
    Declaration::ClassDeclaration(_) => "class-declaration",
    Declaration::TSTypeAliasDeclaration(_) => "type-alias",
    Declaration::TSInterfaceDeclaration(_) => "interface-declaration",
    Declaration::TSEnumDeclaration(_) => "enum-declaration",
    Declaration::TSModuleDeclaration(_) => "module-declaration",
    Declaration::TSGlobalDeclaration(_) => "global-declaration",
    Declaration::TSImportEqualsDeclaration(_) => "import-equals",
);

surface_classifier!(
    module_declaration_surface,
    MODULE_DECLARATION_SURFACES,
    ModuleDeclaration<'_>,
    ModuleDeclaration::ImportDeclaration(_) => "import-declaration",
    ModuleDeclaration::ExportAllDeclaration(_) => "export-all",
    ModuleDeclaration::ExportDefaultDeclaration(_) => "export-default",
    ModuleDeclaration::ExportNamedDeclaration(_) => "export-named",
    ModuleDeclaration::TSExportAssignment(_) => "export-assignment",
    ModuleDeclaration::TSNamespaceExportDeclaration(_) => "namespace-export",
);

surface_classifier!(
    ts_type_surface,
    TS_TYPE_SURFACES,
    TSType<'_>,
    TSType::TSAnyKeyword(_) => "primitive-keyword",
    TSType::TSUnknownKeyword(_) => "primitive-keyword",
    TSType::TSNeverKeyword(_) => "primitive-keyword",
    TSType::TSVoidKeyword(_) => "primitive-keyword",
    TSType::TSNullKeyword(_) => "primitive-keyword",
    TSType::TSUndefinedKeyword(_) => "primitive-keyword",
    TSType::TSBooleanKeyword(_) => "primitive-keyword",
    TSType::TSNumberKeyword(_) => "primitive-keyword",
    TSType::TSStringKeyword(_) => "primitive-keyword",
    TSType::TSBigIntKeyword(_) => "bigint-keyword",
    TSType::TSIntrinsicKeyword(_) => "intrinsic-keyword",
    TSType::TSObjectKeyword(_) => "object-keyword",
    TSType::TSSymbolKeyword(_) => "symbol-keyword",
    TSType::TSThisType(_) => "this-type",
    TSType::TSArrayType(_) => "array-type",
    TSType::TSConditionalType(_) => "conditional-type",
    TSType::TSConstructorType(_) => "constructor-type",
    TSType::TSFunctionType(_) => "function-type",
    TSType::TSImportType(_) => "import-type",
    TSType::TSIndexedAccessType(_) => "indexed-access-type",
    TSType::TSInferType(_) => "infer-type",
    TSType::TSIntersectionType(_) => "intersection-type",
    TSType::TSLiteralType(_) => "literal-type",
    TSType::TSMappedType(_) => "mapped-type",
    TSType::TSNamedTupleMember(_) => "named-tuple-member",
    TSType::TSTemplateLiteralType(_) => "template-literal-type",
    TSType::TSTupleType(_) => "tuple-type",
    TSType::TSTypeLiteral(_) => "type-literal",
    TSType::TSTypeOperatorType(_) => "type-operator",
    TSType::TSTypePredicate(_) => "type-predicate",
    TSType::TSTypeQuery(_) => "type-query",
    TSType::TSTypeReference(_) => "type-reference",
    TSType::TSUnionType(_) => "union-type",
    TSType::TSParenthesizedType(_) => "parenthesized-type",
    TSType::JSDocNullableType(_) => "jsdoc-type",
    TSType::JSDocNonNullableType(_) => "jsdoc-type",
    TSType::JSDocUnknownType(_) => "jsdoc-type",
);

surface_classifier!(
    ts_signature_surface,
    TS_SIGNATURE_SURFACES,
    TSSignature<'_>,
    TSSignature::TSIndexSignature(_) => "index-signature",
    TSSignature::TSPropertySignature(_) => "property-signature",
    TSSignature::TSCallSignatureDeclaration(_) => "call-signature",
    TSSignature::TSConstructSignatureDeclaration(_) => "construct-signature",
    TSSignature::TSMethodSignature(_) => "method-signature",
);

surface_classifier!(
    class_element_surface,
    CLASS_ELEMENT_SURFACES,
    ClassElement<'_>,
    ClassElement::StaticBlock(_) => "static-block",
    ClassElement::MethodDefinition(_) => "method-definition",
    ClassElement::PropertyDefinition(_) => "property-definition",
    ClassElement::AccessorProperty(_) => "accessor-property",
    ClassElement::TSIndexSignature(_) => "class-index-signature",
);

surface_classifier!(
    object_property_surface,
    OBJECT_PROPERTY_SURFACES,
    ObjectPropertyKind<'_>,
    ObjectPropertyKind::ObjectProperty(_) => "object-literal",
    ObjectPropertyKind::SpreadProperty(_) => "object-literal",
);

surface_classifier!(
    binding_pattern_surface,
    BINDING_PATTERN_SURFACES,
    BindingPattern<'_>,
    BindingPattern::BindingIdentifier(_) => "binding-pattern",
    BindingPattern::ObjectPattern(_) => "binding-pattern",
    BindingPattern::ArrayPattern(_) => "binding-pattern",
    BindingPattern::AssignmentPattern(_) => "binding-pattern",
);

surface_classifier!(
    ts_type_name_surface,
    TS_TYPE_NAME_SURFACES,
    TSTypeName<'_>,
    TSTypeName::IdentifierReference(_) => "type-name",
    TSTypeName::QualifiedName(_) => "type-name",
    TSTypeName::ThisExpression(_) => "type-name",
);

surface_classifier!(
    ts_literal_surface,
    TS_LITERAL_SURFACES,
    TSLiteral<'_>,
    TSLiteral::BooleanLiteral(_) => "literal-type",
    TSLiteral::NumericLiteral(_) => "literal-type",
    TSLiteral::BigIntLiteral(_) => "literal-type",
    TSLiteral::StringLiteral(_) => "literal-type",
    TSLiteral::TemplateLiteral(_) => "literal-type",
    TSLiteral::UnaryExpression(_) => "literal-type",
);

/// `TSTupleElement` = own variants + inherited `TSType` variants. A new own variant
/// breaks this match; a new inherited variant breaks `ts_type_surface`.
pub fn ts_tuple_element_surface(node: &TSTupleElement<'_>) -> &'static str {
    match node {
        TSTupleElement::TSOptionalType(_) => "tuple-optional-element",
        TSTupleElement::TSRestType(_) => "tuple-rest-element",
        it @ match_ts_type!(TSTupleElement) => ts_type_surface(it.to_ts_type()),
    }
}
pub const TS_TUPLE_ELEMENT_SURFACES: &[&str] = &["tuple-optional-element", "tuple-rest-element"];

/// `ForStatementInit` = own variants + inherited `Expression` variants. A new own variant
/// breaks this match; a new inherited variant breaks `expression_surface`.
pub fn for_statement_init_surface(node: &ForStatementInit<'_>) -> &'static str {
    match node {
        ForStatementInit::VariableDeclaration(_) => "variable-declaration",
        it @ match_expression!(ForStatementInit) => expression_surface(it.to_expression()),
    }
}
pub const FOR_STATEMENT_INIT_SURFACES: &[&str] = &["variable-declaration"];

/// `ForStatementLeft` = own variants + inherited `AssignmentTarget` variants. A new own variant
/// breaks this match; the inherited group maps to one stmt-check-owned family.
pub fn for_statement_left_surface(node: &ForStatementLeft<'_>) -> &'static str {
    match node {
        ForStatementLeft::VariableDeclaration(_) => "variable-declaration",
        match_assignment_target!(ForStatementLeft) => "assignment-target",
    }
}
pub const FOR_STATEMENT_LEFT_SURFACES: &[&str] = &["assignment-target", "variable-declaration"];

/// Every classifier-emitted surface family paired with the dispatcher role that
/// must carry its inventory record. The validator requires one `[[surface]]`
/// record with this `surface` under this `role` for each pair — a family filed
/// under the wrong role does not count as coverage.
pub const FAMILY_ROLES: &[(&str, &str)] = &[
    ("accessor-property", "class"),
    ("array-literal", "expr-infer"),
    ("array-type", "annotation-lower"),
    ("arrow-function", "expr-infer"),
    ("as-assertion", "expr-infer"),
    ("assignment-expression", "expr-infer"),
    ("assignment-target", "stmt-check"),
    ("await-expression", "expr-infer"),
    ("bigint-keyword", "annotation-lower"),
    ("bigint-literal", "expr-infer"),
    ("binary-expression", "expr-infer"),
    ("binding-pattern", "bind"),
    ("block-statement", "stmt-check"),
    ("break-statement", "stmt-check"),
    ("call-expression", "expr-infer"),
    ("call-signature", "signature"),
    ("class-declaration", "decl"),
    ("class-expression", "expr-infer"),
    ("class-index-signature", "class"),
    ("conditional-expression", "expr-infer"),
    ("conditional-type", "annotation-lower"),
    ("construct-signature", "signature"),
    ("constructor-type", "annotation-lower"),
    ("continue-statement", "stmt-check"),
    ("debugger-statement", "stmt-check"),
    ("do-while-statement", "stmt-check"),
    ("element-access", "expr-infer"),
    ("empty-statement", "stmt-check"),
    ("enum-declaration", "decl"),
    ("export-all", "decl"),
    ("export-assignment", "decl"),
    ("export-default", "decl"),
    ("export-named", "decl"),
    ("expression-statement", "stmt-check"),
    ("for-in-statement", "stmt-check"),
    ("for-of-statement", "stmt-check"),
    ("for-statement", "stmt-check"),
    ("function-declaration", "decl"),
    ("function-expression", "expr-infer"),
    ("function-type", "annotation-lower"),
    ("global-declaration", "decl"),
    ("identifier-reference", "expr-infer"),
    ("if-statement", "stmt-check"),
    ("import-declaration", "decl"),
    ("import-equals", "decl"),
    ("import-expression", "expr-infer"),
    ("import-type", "annotation-lower"),
    ("index-signature", "signature"),
    ("indexed-access-type", "annotation-lower"),
    ("infer-type", "annotation-lower"),
    ("instantiation-expression", "expr-infer"),
    ("interface-declaration", "decl"),
    ("intersection-type", "annotation-lower"),
    ("intrinsic-keyword", "annotation-lower"),
    ("jsdoc-type", "annotation-lower"),
    ("jsx", "expr-infer"),
    ("labeled-statement", "stmt-check"),
    ("literal", "expr-infer"),
    ("literal-type", "annotation-lower"),
    ("logical-expression", "expr-infer"),
    ("mapped-type", "annotation-lower"),
    ("member-access", "expr-infer"),
    ("meta-property", "expr-infer"),
    ("method-definition", "class"),
    ("method-signature", "signature"),
    ("module-declaration", "decl"),
    ("named-tuple-member", "annotation-lower"),
    ("namespace-export", "decl"),
    ("new-expression", "expr-infer"),
    ("non-null-assertion", "expr-infer"),
    ("object-keyword", "annotation-lower"),
    ("object-literal", "expr-infer"),
    ("optional-chain", "expr-infer"),
    ("parenthesized-expression", "expr-infer"),
    ("parenthesized-type", "annotation-lower"),
    ("primitive-keyword", "annotation-lower"),
    ("private-field-access", "expr-infer"),
    ("private-in-expression", "expr-infer"),
    ("property-definition", "class"),
    ("property-signature", "signature"),
    ("regexp-literal", "expr-infer"),
    ("return-statement", "stmt-check"),
    ("satisfies-expression", "expr-infer"),
    ("sequence-expression", "expr-infer"),
    ("static-block", "class"),
    ("super", "expr-infer"),
    ("switch-statement", "stmt-check"),
    ("symbol-keyword", "annotation-lower"),
    ("tagged-template", "expr-infer"),
    ("template-literal", "expr-infer"),
    ("template-literal-type", "annotation-lower"),
    ("this-expression", "expr-infer"),
    ("this-type", "annotation-lower"),
    ("throw-statement", "stmt-check"),
    ("try-statement", "stmt-check"),
    ("tuple-optional-element", "annotation-lower"),
    ("tuple-rest-element", "annotation-lower"),
    ("tuple-type", "annotation-lower"),
    ("type-alias", "decl"),
    ("type-assertion", "expr-infer"),
    ("type-literal", "annotation-lower"),
    ("type-name", "annotation-lower"),
    ("type-operator", "annotation-lower"),
    ("type-predicate", "annotation-lower"),
    ("type-query", "annotation-lower"),
    ("type-reference", "annotation-lower"),
    ("unary-expression", "expr-infer"),
    ("union-type", "annotation-lower"),
    ("update-expression", "expr-infer"),
    ("v8-intrinsic", "expr-infer"),
    ("variable-declaration", "decl"),
    ("while-statement", "stmt-check"),
    ("with-statement", "stmt-check"),
    ("yield-expression", "expr-infer"),
];

/// The ten dispatcher roles the inventory must classify (census, WU0).
pub const SURFACE_ROLES: &[&str] = &[
    "bind",
    "flow",
    "type-fill",
    "stmt-check",
    "expr-infer",
    "annotation-lower",
    "call",
    "decl",
    "class",
    "signature",
];

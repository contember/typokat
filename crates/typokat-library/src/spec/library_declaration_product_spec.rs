//! WU2 acceptance contract for the immutable full-profile declaration product.

use super::profile::ExactLibraryProfile;
use crate::binder::declaration::{
    DeclarationKind, SourceDeclarationManifest, SourceDeclarationManifestBuildScopeForTest,
    SourceInterfaceOccurrenceKind, SourceTypeFragmentPlacement, TypeFragmentKind,
};
use crate::binder::namespace::{source_file_kind, ModuleBindingContext};
use crate::check::checker::library_compiler::{
    compile_library_binder_checkpoint, InjectedLibrarySource,
};
use oxc_allocator::Allocator;
use oxc_ast::ast::{
    BindingPattern, MethodDefinitionKind, PropertyKey, TSLiteral, TSMethodSignatureKind,
    TSTypeOperatorOperator, VariableDeclarationKind,
};
use oxc_ast::AstKind;
use oxc_ast_visit::Visit;
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType, Span};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const PROFILE_SHA256: &str = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d";
const RAW_CONCAT_SHA256: &str = "0c68516cfe1dff30ce17425b2566813cf6d00c7f589dd24f31f4ba879b69a267";
const EXPECTED_CENSUS: &str = include_str!("../library-declaration-product-census.txt");
const FORBIDDEN_PRODUCT_TOKENS: &[&str] = &[
    "oxc_ast",
    "Allocator",
    "Binder",
    "ScopeId",
    "DeclId",
    "TypeGroupId",
    "ValueStorageId",
    "SymbolId",
    "ClassId",
    "TypeId",
    "TypeParamId",
    "DeclaredRecipeId",
    "LibraryBinderCheckpoint",
    "Store",
    "Interner",
    "Event",
    "GlobalObject",
    "Cache",
    "ReplayOwner",
    "Fingerprint",
    "UnsafeCell",
    "RefCell",
    "Cell<",
    "Mutex",
    "RwLock",
    "Atomic",
    "OnceLock",
    "LazyLock",
    "dyn Any",
    "include!",
    "include_bytes!",
    "include_str!",
    "OUT_DIR",
    "Box<dyn",
    "raw_source",
    "opaque_blob",
];

const COUNTED_AST_KINDS: &[&str] = &[
    "AccessorProperty",
    "ArrayPattern",
    "AssignmentPattern",
    "BigIntLiteral",
    "BindingIdentifier",
    "BindingRestElement",
    "BooleanLiteral",
    "Class",
    "ExportAllDeclaration",
    "ExportDefaultDeclaration",
    "ExportNamedDeclaration",
    "FormalParameter",
    "FormalParameterRest",
    "Function",
    "ImportDeclaration",
    "JSDocNonNullableType",
    "JSDocNullableType",
    "JSDocUnknownType",
    "MethodDefinition",
    "NullLiteral",
    "NumericLiteral",
    "ObjectPattern",
    "PropertyDefinition",
    "StaticBlock",
    "StringLiteral",
    "TSAnyKeyword",
    "TSArrayType",
    "TSBigIntKeyword",
    "TSBooleanKeyword",
    "TSCallSignatureDeclaration",
    "TSConditionalType",
    "TSConstructSignatureDeclaration",
    "TSConstructorType",
    "TSEnumDeclaration",
    "TSEnumMember",
    "TSExportAssignment",
    "TSFunctionType",
    "TSGlobalDeclaration",
    "TSImportEqualsDeclaration",
    "TSImportType",
    "TSImportTypeQualifiedName",
    "TSIndexSignature",
    "TSIndexedAccessType",
    "TSInferType",
    "TSInterfaceDeclaration",
    "TSInterfaceHeritage",
    "TSIntersectionType",
    "TSIntrinsicKeyword",
    "TSLiteralType",
    "TSMappedType",
    "TSMethodSignature",
    "TSModuleBlock",
    "TSModuleDeclaration",
    "TSNamedTupleMember",
    "TSNamespaceExportDeclaration",
    "TSNeverKeyword",
    "TSNullKeyword",
    "TSNumberKeyword",
    "TSObjectKeyword",
    "TSOptionalType",
    "TSParenthesizedType",
    "TSPropertySignature",
    "TSQualifiedName",
    "TSRestType",
    "TSStringKeyword",
    "TSSymbolKeyword",
    "TSTemplateLiteralType",
    "TSThisParameter",
    "TSThisType",
    "TSTupleType",
    "TSTypeAliasDeclaration",
    "TSTypeLiteral",
    "TSTypeOperator",
    "TSTypeParameter",
    "TSTypeParameterDeclaration",
    "TSTypeParameterInstantiation",
    "TSTypePredicate",
    "TSTypeQuery",
    "TSTypeReference",
    "TSUndefinedKeyword",
    "TSUnionType",
    "TSUnknownKeyword",
    "TSVoidKeyword",
    "VariableDeclaration",
    "VariableDeclarator",
    "form.method.computed",
    "form.method.constructor",
    "form.method.get",
    "form.method.method",
    "form.method.optional",
    "form.method.override",
    "form.method.set",
    "form.method.static",
    "form.parameter.accessibility",
    "form.parameter.array_pattern",
    "form.parameter.assignment_pattern",
    "form.parameter.identifier",
    "form.parameter.initializer",
    "form.parameter.object_pattern",
    "form.parameter.optional",
    "form.parameter.override",
    "form.parameter.readonly",
    "form.property.computed",
    "form.property.optional",
    "form.property.readonly",
    "form.property_key.computed",
    "form.property_key.identifier",
    "form.property_key.literal",
    "form.property_key.private",
    "form.type_operator.keyof",
    "form.type_operator.readonly",
    "form.type_operator.unique",
    "form.ts_literal.bigint",
    "form.ts_literal.boolean",
    "form.ts_literal.numeric",
    "form.ts_literal.other",
    "form.ts_literal.string",
    "form.ts_literal.template",
    "form.ts_literal.unary_minus",
    "form.variable.await_using",
    "form.variable.const",
    "form.variable.let",
    "form.variable.using",
    "form.variable.var",
];

#[derive(Default)]
struct AstCensus {
    counts: BTreeMap<&'static str, usize>,
    source_ordinal: usize,
    site_rows: Vec<String>,
    dependency_rows: Vec<String>,
}

impl AstCensus {
    fn new() -> Self {
        Self {
            counts: COUNTED_AST_KINDS.iter().map(|kind| (*kind, 0)).collect(),
            source_ordinal: 0,
            site_rows: Vec::new(),
            dependency_rows: Vec::new(),
        }
    }

    fn record(&mut self, kind: &'static str) {
        *self.counts.get_mut(kind).expect("counted AST kind") += 1;
    }

    fn record_node(&mut self, kind: &'static str, span: Span) {
        self.record(kind);
        self.site_rows.push(format!(
            "{}|{}|{}|{kind}",
            self.source_ordinal, span.start, span.end
        ));
        if matches!(
            kind,
            "TSTypeReference"
                | "TSInterfaceHeritage"
                | "TSTypeQuery"
                | "TSImportType"
                | "TSIndexedAccessType"
        ) {
            self.dependency_rows.push(format!(
                "{}|{}|{}|{kind}",
                self.source_ordinal, span.start, span.end
            ));
        }
    }
}

impl<'ast> Visit<'ast> for AstCensus {
    fn enter_node(&mut self, kind: AstKind<'ast>) {
        let span = kind.span();
        let name = match kind {
            AstKind::AccessorProperty(_) => "AccessorProperty",
            AstKind::ArrayPattern(_) => "ArrayPattern",
            AstKind::AssignmentPattern(_) => "AssignmentPattern",
            AstKind::BigIntLiteral(_) => "BigIntLiteral",
            AstKind::BindingIdentifier(_) => "BindingIdentifier",
            AstKind::BindingRestElement(_) => "BindingRestElement",
            AstKind::BooleanLiteral(_) => "BooleanLiteral",
            AstKind::Class(_) => "Class",
            AstKind::ExportAllDeclaration(_) => "ExportAllDeclaration",
            AstKind::ExportDefaultDeclaration(_) => "ExportDefaultDeclaration",
            AstKind::ExportNamedDeclaration(_) => "ExportNamedDeclaration",
            AstKind::FormalParameter(parameter) => {
                self.record(parameter_pattern_name(&parameter.pattern));
                if parameter.optional {
                    self.record("form.parameter.optional");
                }
                if parameter.initializer.is_some() {
                    self.record("form.parameter.initializer");
                }
                if parameter.accessibility.is_some() {
                    self.record("form.parameter.accessibility");
                }
                if parameter.readonly {
                    self.record("form.parameter.readonly");
                }
                if parameter.r#override {
                    self.record("form.parameter.override");
                }
                "FormalParameter"
            }
            AstKind::FormalParameterRest(_) => "FormalParameterRest",
            AstKind::Function(_) => "Function",
            AstKind::ImportDeclaration(_) => "ImportDeclaration",
            AstKind::JSDocNonNullableType(_) => "JSDocNonNullableType",
            AstKind::JSDocNullableType(_) => "JSDocNullableType",
            AstKind::JSDocUnknownType(_) => "JSDocUnknownType",
            AstKind::MethodDefinition(method) => {
                self.record(method_kind_name(method.kind));
                self.record(property_key_name(&method.key, method.computed));
                if method.computed {
                    self.record("form.method.computed");
                }
                if method.r#static {
                    self.record("form.method.static");
                }
                if method.optional {
                    self.record("form.method.optional");
                }
                if method.r#override {
                    self.record("form.method.override");
                }
                "MethodDefinition"
            }
            AstKind::NullLiteral(_) => "NullLiteral",
            AstKind::NumericLiteral(_) => "NumericLiteral",
            AstKind::ObjectPattern(_) => "ObjectPattern",
            AstKind::PropertyDefinition(property) => {
                self.record(property_key_name(&property.key, property.computed));
                "PropertyDefinition"
            }
            AstKind::StaticBlock(_) => "StaticBlock",
            AstKind::StringLiteral(_) => "StringLiteral",
            AstKind::TSAnyKeyword(_) => "TSAnyKeyword",
            AstKind::TSArrayType(_) => "TSArrayType",
            AstKind::TSBigIntKeyword(_) => "TSBigIntKeyword",
            AstKind::TSBooleanKeyword(_) => "TSBooleanKeyword",
            AstKind::TSCallSignatureDeclaration(_) => "TSCallSignatureDeclaration",
            AstKind::TSConditionalType(_) => "TSConditionalType",
            AstKind::TSConstructSignatureDeclaration(_) => "TSConstructSignatureDeclaration",
            AstKind::TSConstructorType(_) => "TSConstructorType",
            AstKind::TSEnumDeclaration(_) => "TSEnumDeclaration",
            AstKind::TSEnumMember(_) => "TSEnumMember",
            AstKind::TSExportAssignment(_) => "TSExportAssignment",
            AstKind::TSFunctionType(_) => "TSFunctionType",
            AstKind::TSGlobalDeclaration(_) => "TSGlobalDeclaration",
            AstKind::TSImportEqualsDeclaration(_) => "TSImportEqualsDeclaration",
            AstKind::TSImportType(_) => "TSImportType",
            AstKind::TSImportTypeQualifiedName(_) => "TSImportTypeQualifiedName",
            AstKind::TSIndexSignature(_) => "TSIndexSignature",
            AstKind::TSIndexedAccessType(_) => "TSIndexedAccessType",
            AstKind::TSInferType(_) => "TSInferType",
            AstKind::TSInterfaceDeclaration(_) => "TSInterfaceDeclaration",
            AstKind::TSInterfaceHeritage(_) => "TSInterfaceHeritage",
            AstKind::TSIntersectionType(_) => "TSIntersectionType",
            AstKind::TSIntrinsicKeyword(_) => "TSIntrinsicKeyword",
            AstKind::TSLiteralType(literal) => {
                self.record(ts_literal_kind_name(&literal.literal));
                "TSLiteralType"
            }
            AstKind::TSMappedType(_) => "TSMappedType",
            AstKind::TSMethodSignature(method) => {
                self.record(ts_method_kind_name(method.kind));
                self.record(property_key_name(&method.key, method.computed));
                if method.computed {
                    self.record("form.method.computed");
                }
                if method.optional {
                    self.record("form.method.optional");
                }
                "TSMethodSignature"
            }
            AstKind::TSModuleBlock(_) => "TSModuleBlock",
            AstKind::TSModuleDeclaration(_) => "TSModuleDeclaration",
            AstKind::TSNamedTupleMember(_) => "TSNamedTupleMember",
            AstKind::TSNamespaceExportDeclaration(_) => "TSNamespaceExportDeclaration",
            AstKind::TSNeverKeyword(_) => "TSNeverKeyword",
            AstKind::TSNullKeyword(_) => "TSNullKeyword",
            AstKind::TSNumberKeyword(_) => "TSNumberKeyword",
            AstKind::TSObjectKeyword(_) => "TSObjectKeyword",
            AstKind::TSOptionalType(_) => "TSOptionalType",
            AstKind::TSParenthesizedType(_) => "TSParenthesizedType",
            AstKind::TSPropertySignature(property) => {
                self.record(property_key_name(&property.key, property.computed));
                if property.computed {
                    self.record("form.property.computed");
                }
                if property.optional {
                    self.record("form.property.optional");
                }
                if property.readonly {
                    self.record("form.property.readonly");
                }
                "TSPropertySignature"
            }
            AstKind::TSQualifiedName(_) => "TSQualifiedName",
            AstKind::TSRestType(_) => "TSRestType",
            AstKind::TSStringKeyword(_) => "TSStringKeyword",
            AstKind::TSSymbolKeyword(_) => "TSSymbolKeyword",
            AstKind::TSTemplateLiteralType(_) => "TSTemplateLiteralType",
            AstKind::TSThisParameter(_) => "TSThisParameter",
            AstKind::TSThisType(_) => "TSThisType",
            AstKind::TSTupleType(_) => "TSTupleType",
            AstKind::TSTypeAliasDeclaration(_) => "TSTypeAliasDeclaration",
            AstKind::TSTypeLiteral(_) => "TSTypeLiteral",
            AstKind::TSTypeOperator(operator) => {
                self.record(type_operator_name(operator.operator));
                "TSTypeOperator"
            }
            AstKind::TSTypeParameter(_) => "TSTypeParameter",
            AstKind::TSTypeParameterDeclaration(_) => "TSTypeParameterDeclaration",
            AstKind::TSTypeParameterInstantiation(_) => "TSTypeParameterInstantiation",
            AstKind::TSTypePredicate(_) => "TSTypePredicate",
            AstKind::TSTypeQuery(_) => "TSTypeQuery",
            AstKind::TSTypeReference(_) => "TSTypeReference",
            AstKind::TSUndefinedKeyword(_) => "TSUndefinedKeyword",
            AstKind::TSUnionType(_) => "TSUnionType",
            AstKind::TSUnknownKeyword(_) => "TSUnknownKeyword",
            AstKind::TSVoidKeyword(_) => "TSVoidKeyword",
            AstKind::VariableDeclaration(declaration) => {
                self.record(variable_kind_name(declaration.kind));
                "VariableDeclaration"
            }
            AstKind::VariableDeclarator(_) => "VariableDeclarator",
            _ => return,
        };
        self.record_node(name, span);
    }
}

fn parameter_pattern_name(pattern: &BindingPattern<'_>) -> &'static str {
    match pattern {
        BindingPattern::BindingIdentifier(_) => "form.parameter.identifier",
        BindingPattern::ObjectPattern(_) => "form.parameter.object_pattern",
        BindingPattern::ArrayPattern(_) => "form.parameter.array_pattern",
        BindingPattern::AssignmentPattern(_) => "form.parameter.assignment_pattern",
    }
}

fn method_kind_name(kind: MethodDefinitionKind) -> &'static str {
    match kind {
        MethodDefinitionKind::Constructor => "form.method.constructor",
        MethodDefinitionKind::Method => "form.method.method",
        MethodDefinitionKind::Get => "form.method.get",
        MethodDefinitionKind::Set => "form.method.set",
    }
}

fn ts_method_kind_name(kind: TSMethodSignatureKind) -> &'static str {
    match kind {
        TSMethodSignatureKind::Method => "form.method.method",
        TSMethodSignatureKind::Get => "form.method.get",
        TSMethodSignatureKind::Set => "form.method.set",
    }
}

fn property_key_name(key: &PropertyKey<'_>, computed: bool) -> &'static str {
    if computed {
        "form.property_key.computed"
    } else if key.is_private_identifier() {
        "form.property_key.private"
    } else if key.is_identifier() {
        "form.property_key.identifier"
    } else {
        "form.property_key.literal"
    }
}

fn type_operator_name(operator: TSTypeOperatorOperator) -> &'static str {
    match operator {
        TSTypeOperatorOperator::Keyof => "form.type_operator.keyof",
        TSTypeOperatorOperator::Unique => "form.type_operator.unique",
        TSTypeOperatorOperator::Readonly => "form.type_operator.readonly",
    }
}

fn ts_literal_kind_name(literal: &TSLiteral<'_>) -> &'static str {
    match literal {
        TSLiteral::StringLiteral(_) => "form.ts_literal.string",
        TSLiteral::NumericLiteral(_) => "form.ts_literal.numeric",
        TSLiteral::BooleanLiteral(_) => "form.ts_literal.boolean",
        TSLiteral::BigIntLiteral(_) => "form.ts_literal.bigint",
        TSLiteral::TemplateLiteral(_) => "form.ts_literal.template",
        TSLiteral::UnaryExpression(expression) if expression.operator.as_str() == "-" => {
            "form.ts_literal.unary_minus"
        }
        TSLiteral::UnaryExpression(_) => "form.ts_literal.other",
    }
}

fn variable_kind_name(kind: VariableDeclarationKind) -> &'static str {
    match kind {
        VariableDeclarationKind::Var => "form.variable.var",
        VariableDeclarationKind::Let => "form.variable.let",
        VariableDeclarationKind::Const => "form.variable.const",
        VariableDeclarationKind::Using => "form.variable.using",
        VariableDeclarationKind::AwaitUsing => "form.variable.await_using",
    }
}

#[test]
fn full_profile_source_and_binding_census_is_frozen() {
    let actual = collect_census();
    assert_eq!(
        actual, EXPECTED_CENSUS,
        "full-profile census drifted\n{actual}"
    );
}

#[test]
fn ownership_source_audit_rejects_the_known_broken_control() {
    let broken = include_str!("../library-declaration-product-forbidden-control.txt");
    assert_eq!(forbidden_product_tokens(broken), FORBIDDEN_PRODUCT_TOKENS);
}

#[test]
fn candidate_product_crate_has_the_closed_dependency_free_boundary() {
    let root = workspace_root();
    let crate_root = root.join("crates/typokat-library-product");
    let manifest = fs::read_to_string(crate_root.join("Cargo.toml"))
        .expect("candidate product crate manifest");
    assert!(!manifest.contains("oxc_"));
    assert!(!manifest.contains("[dependencies]"));
    assert!(!crate_root.join("build.rs").exists());

    let metadata = Command::new("cargo")
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--manifest-path",
        ])
        .arg(crate_root.join("Cargo.toml"))
        .output()
        .expect("candidate cargo metadata");
    assert!(metadata.status.success(), "candidate cargo metadata failed");
    let metadata: serde_json::Value =
        serde_json::from_slice(&metadata.stdout).expect("candidate metadata JSON");
    let package = metadata["packages"]
        .as_array()
        .expect("metadata packages")
        .iter()
        .find(|package| package["name"] == "typokat-library-product")
        .expect("candidate package metadata");
    assert_eq!(package["dependencies"].as_array().map(Vec::len), Some(0));
    assert!(package["targets"]
        .as_array()
        .expect("candidate targets")
        .iter()
        .all(|target| target["kind"]
            .as_array()
            .is_some_and(|kinds| !kinds.iter().any(|kind| kind == "custom-build"))));

    assert_eq!(
        rust_source_paths(&crate_root),
        [
            "src/canonical.rs",
            "src/keys.rs",
            "src/lib.rs",
            "src/schema.rs",
            "src/validate.rs",
            "src/walk.rs",
            "tests/forbidden_schema.rs",
        ]
    );

    let source = read_rust_tree(&crate_root.join("src"));
    for required in [
        "LibraryDeclarationProduct",
        "ProductFieldClass",
        "ProductFieldVisitor",
        "SourceKey",
        "DeclarationKey",
        "GroupKey",
        "TypeRecipeKey",
        "ProductAuditReceipt",
        "dependency_edge_digest",
        "dependency_scc_digest",
        "lexical_order_digest",
        "namespace_attachment_digest",
        "value_attachment_digest",
        "validate",
        "walk_fields",
    ] {
        assert!(
            source.contains(required),
            "missing product contract: {required}"
        );
    }
    assert_eq!(forbidden_product_tokens(&source), Vec::<&str>::new());

    let product_tests =
        fs::read_to_string(root.join(
            "crates/typokat-library/src/spec/library_declaration_product_integration_spec.rs",
        ))
        .expect("library-owned candidate full-profile tests");
    for required_test in [
        "product_receipt_matches_authenticated_edge_and_attachment_oracles",
        "product_builds_one_manifest_per_source",
        "product_drops_ast_allocator_and_binder",
        "product_is_repeat_reverse_shuffle_and_process_deterministic",
        "product_rejects_forbidden_schema_controls",
    ] {
        assert!(
            product_tests.contains(required_test),
            "missing candidate lifecycle witness: {required_test}"
        );
    }
}

fn collect_census() -> String {
    let profile = ExactLibraryProfile::load_packaged().expect("packaged full-library profile");
    let mut counts = BTreeMap::<String, usize>::new();
    let mut facts = BTreeMap::<String, String>::new();
    let mut declaration_rows = Vec::new();
    let mut namespace_attachment_rows = Vec::new();
    let mut value_attachment_rows = Vec::new();
    let mut interface_lexical_rows = Vec::new();
    for name in [
        "declaration.Variable",
        "declaration.Function",
        "declaration.Class",
        "declaration.Parameter",
        "declaration.CatchParameter",
        "declaration.Import",
        "declaration.TypeAlias",
        "declaration.Interface",
        "declaration.Enum",
        "declaration.Namespace",
        "declaration.ImportEquals",
        "declaration.NamespaceExport",
        "declaration.Global",
        "declaration.DefaultExportExpression",
        "fragment.TypeAlias",
        "fragment.Interface",
        "fragment.Class",
        "fragment.Enum",
        "placement.Ordinary",
        "placement.ClassExpression",
        "placement.DefaultClass",
        "placement.UnadmittedClassStaticBlock",
        "placement.UnadmittedAmbientExternalModule",
        "placement.GlobalAugmentationBody",
        "interface_occurrence.Header",
        "interface_occurrence.Heritage",
        "interface_occurrence.Member",
        "source.external_module",
        "source.script",
    ] {
        counts.insert(name.to_owned(), 0);
    }
    let mut ast = AstCensus::new();
    let mut source_bytes = 0usize;
    let manifest_scope = SourceDeclarationManifestBuildScopeForTest::start();

    for source in profile.sources() {
        ast.source_ordinal = source.ordinal().index();
        source_bytes += source.bytes().len();
        let allocator = Allocator::default();
        let text = std::str::from_utf8(source.bytes()).expect("packaged source is UTF-8");
        let parsed = Parser::new(&allocator, text, SourceType::d_ts()).parse();
        assert!(
            parsed.diagnostics.is_empty(),
            "{} parse errors: {:?}",
            source.name(),
            parsed.diagnostics
        );
        let context =
            ModuleBindingContext::for_program(&parsed.program, source_file_kind(source.name()));
        bump(
            &mut counts,
            if context.external_module {
                "source.external_module"
            } else {
                "source.script"
            },
            1,
        );

        let manifest = SourceDeclarationManifest::build(&parsed.program);
        for declaration in manifest.declarations() {
            let name = text
                .get(
                    usize::try_from(declaration.binding_span.start).expect("span start fits usize")
                        ..usize::try_from(declaration.binding_span.end)
                            .expect("span end fits usize"),
                )
                .unwrap_or("<non-utf8-boundary>");
            let row = format!(
                "{}|{}|{}:{}|{}:{}|{name}",
                source.ordinal().index(),
                declaration_kind_name(declaration.kind),
                declaration.declaration_span.start,
                declaration.declaration_span.end,
                declaration.binding_span.start,
                declaration.binding_span.end,
            );
            declaration_rows.push(row.clone());
            if matches!(
                declaration.kind,
                DeclarationKind::Variable
                    | DeclarationKind::Function
                    | DeclarationKind::Class
                    | DeclarationKind::Enum
                    | DeclarationKind::Namespace
            ) {
                value_attachment_rows.push(row.clone());
            }
            if declaration.kind == DeclarationKind::Namespace {
                namespace_attachment_rows.push(row);
            }
            bump(&mut counts, declaration_kind_name(declaration.kind), 1);
        }
        for fragment in manifest.type_fragments() {
            bump(
                &mut counts,
                type_fragment_kind_name(fragment.payload.kind()),
                1,
            );
            bump(
                &mut counts,
                type_fragment_placement_name(fragment.placement),
                1,
            );
            for occurrence in manifest.interface_occurrences(fragment.interface_occurrences.clone())
            {
                interface_lexical_rows.push(format!(
                    "{}|{}|{}:{}",
                    source.ordinal().index(),
                    interface_occurrence_name(occurrence.kind),
                    occurrence.span.start,
                    occurrence.span.end,
                ));
                bump(&mut counts, interface_occurrence_name(occurrence.kind), 1);
            }
        }
        // This second walk is an independent test oracle. Product construction may not repeat it.
        ast.visit_program(&parsed.program);
    }
    counts.insert(
        "oracle.census.manifest_builds".to_owned(),
        usize::try_from(manifest_scope.finish()).expect("manifest count fits usize"),
    );

    record_rows_fact(&mut facts, "oracle.ast_site_rows", &ast.site_rows);
    record_rows_fact(
        &mut facts,
        "oracle.candidate_dependency_sites",
        &ast.dependency_rows,
    );
    record_rows_fact(&mut facts, "oracle.declaration_rows", &declaration_rows);
    record_rows_fact(
        &mut facts,
        "oracle.candidate_namespace_sites",
        &namespace_attachment_rows,
    );
    record_rows_fact(
        &mut facts,
        "oracle.candidate_value_sites",
        &value_attachment_rows,
    );
    record_rows_fact(
        &mut facts,
        "oracle.interface_lexical_rows",
        &interface_lexical_rows,
    );

    for (kind, count) in ast.counts {
        counts.insert(format!("ast.{kind}"), count);
    }

    let injected = injected_sources(&profile);
    let checkpoint = compile_library_binder_checkpoint(&injected)
        .expect("canonical full-profile binder checkpoint");
    let ordering = checkpoint.type_group_fragment_ordering_for_test();
    let attachment_rows = checkpoint.declaration_attachment_rows_for_test();
    let normalized_attachment_rows = attachment_rows
        .iter()
        .map(|row| {
            format!(
                "{}|{}|{}|{}|value={}|type={}|namespace={}",
                row.library_ordinal.index(),
                declaration_kind_name(row.kind),
                row.declaration_start,
                row.binding_start,
                row.has_value_storage,
                row.type_group_name.as_deref().unwrap_or("-"),
                row.has_namespace,
            )
        })
        .collect::<Vec<_>>();
    let authenticated_value_rows = attachment_rows
        .iter()
        .zip(&normalized_attachment_rows)
        .filter(|(row, _)| row.has_value_storage)
        .map(|(_, normalized)| normalized.clone())
        .collect::<Vec<_>>();
    let authenticated_namespace_rows = attachment_rows
        .iter()
        .zip(&normalized_attachment_rows)
        .filter(|(row, _)| row.has_namespace)
        .map(|(_, normalized)| normalized.clone())
        .collect::<Vec<_>>();
    record_rows_fact(
        &mut facts,
        "oracle.current_binder.authenticated_attachment_rows",
        &normalized_attachment_rows,
    );
    record_rows_fact(
        &mut facts,
        "oracle.current_binder.authenticated_value_rows",
        &authenticated_value_rows,
    );
    record_rows_fact(
        &mut facts,
        "oracle.current_binder.authenticated_namespace_rows",
        &authenticated_namespace_rows,
    );
    let inspection = checkpoint.inspection_for_test();
    let ends = inspection.ends;
    counts.insert(
        "oracle.current_binder.library_units".to_owned(),
        inspection.library_units.len(),
    );
    counts.insert("oracle.current_binder.scopes".to_owned(), ends.scopes);
    counts.insert("oracle.current_binder.symbols".to_owned(), ends.symbols);
    counts.insert(
        "oracle.current_binder.declarations".to_owned(),
        ends.declarations,
    );
    counts.insert(
        "oracle.current_binder.type_groups".to_owned(),
        ends.type_groups,
    );
    counts.insert(
        "oracle.current_binder.namespaces".to_owned(),
        ends.namespaces,
    );
    counts.insert(
        "oracle.current_binder.value_storages".to_owned(),
        ends.value_storages,
    );
    counts.insert(
        "oracle.current_binder.next_source".to_owned(),
        ends.next_source,
    );
    counts.insert(
        "oracle.current_binder.ordered_groups".to_owned(),
        ordering.groups.len(),
    );
    counts.insert(
        "oracle.current_binder.group_fragments".to_owned(),
        ordering
            .groups
            .iter()
            .map(|group| group.fragments_in_binder.len())
            .sum(),
    );
    counts.insert(
        "oracle.current_binder.merged_groups".to_owned(),
        ordering
            .groups
            .iter()
            .filter(|group| group.fragments_in_binder.len() > 1)
            .count(),
    );
    counts.insert(
        "oracle.current_binder.max_group_fragments".to_owned(),
        ordering
            .groups
            .iter()
            .map(|group| group.fragments_in_binder.len())
            .max()
            .unwrap_or(0),
    );
    counts.insert(
        "oracle.current_binder.library_order_mismatches".to_owned(),
        ordering
            .groups
            .iter()
            .filter(|group| group.fragments_in_binder != group.fragments_by_library_ordinal)
            .count(),
    );
    let mut group_fragment_rows = Vec::new();
    for group in &ordering.groups {
        for fragment in &group.fragments_by_library_ordinal {
            let ordinal = ordering
                .units
                .iter()
                .find(|unit| unit.source_key == fragment.source_key)
                .map(|unit| unit.library_ordinal.index())
                .expect("group fragment belongs to a library unit");
            group_fragment_rows.push(format!(
                "{}|{}|{}",
                group.name, ordinal, fragment.declaration_start
            ));
        }
    }
    record_rows_fact(
        &mut facts,
        "oracle.current_binder.group_fragment_rows",
        &group_fragment_rows,
    );

    let mut output = String::new();
    output.push_str("schema=1\n");
    output.push_str("typescript_version=6.0.3\n");
    output.push_str("root=lib.es2025.full.d.ts\n");
    output.push_str(&format!("profile_sha256={PROFILE_SHA256}\n"));
    output.push_str(&format!("raw_concat_sha256={RAW_CONCAT_SHA256}\n"));
    output.push_str(&format!("source_files={}\n", profile.sources().len()));
    output.push_str(&format!("source_bytes={source_bytes}\n"));
    output.push_str(
        "oracle.current_binder.note=allocation-local baseline; forbidden from product schema\n",
    );
    for (name, value) in facts {
        output.push_str(&format!("{name}={value}\n"));
    }
    for (name, count) in counts {
        output.push_str(&format!("{name}={count}\n"));
    }
    output
}

fn injected_sources(profile: &ExactLibraryProfile) -> Vec<InjectedLibrarySource<'_>> {
    profile
        .sources()
        .iter()
        .map(|source| InjectedLibrarySource {
            file_ordinal: source.ordinal(),
            name: source.name(),
            source: std::str::from_utf8(source.bytes()).expect("packaged source is UTF-8"),
        })
        .collect()
}

fn bump(counts: &mut BTreeMap<String, usize>, name: &str, amount: usize) {
    *counts.entry(name.to_owned()).or_default() += amount;
}

fn record_rows_fact(facts: &mut BTreeMap<String, String>, name: &str, rows: &[String]) {
    facts.insert(format!("{name}.count"), rows.len().to_string());
    let mut canonical = rows.join("\n");
    canonical.push('\n');
    facts.insert(format!("{name}.sha256"), digest(canonical.as_bytes()));
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn forbidden_product_tokens(source: &str) -> Vec<&'static str> {
    FORBIDDEN_PRODUCT_TOKENS
        .iter()
        .copied()
        .filter(|token| source.contains(token))
        .collect()
}

fn declaration_kind_name(kind: DeclarationKind) -> &'static str {
    match kind {
        DeclarationKind::Variable => "declaration.Variable",
        DeclarationKind::Function => "declaration.Function",
        DeclarationKind::Class => "declaration.Class",
        DeclarationKind::Parameter => "declaration.Parameter",
        DeclarationKind::CatchParameter => "declaration.CatchParameter",
        DeclarationKind::Import => "declaration.Import",
        DeclarationKind::TypeAlias => "declaration.TypeAlias",
        DeclarationKind::Interface => "declaration.Interface",
        DeclarationKind::Enum => "declaration.Enum",
        DeclarationKind::Namespace => "declaration.Namespace",
        DeclarationKind::ImportEquals => "declaration.ImportEquals",
        DeclarationKind::NamespaceExport => "declaration.NamespaceExport",
        DeclarationKind::Global => "declaration.Global",
        DeclarationKind::DefaultExportExpression => "declaration.DefaultExportExpression",
    }
}

fn type_fragment_kind_name(kind: TypeFragmentKind) -> &'static str {
    match kind {
        TypeFragmentKind::TypeAlias => "fragment.TypeAlias",
        TypeFragmentKind::Interface => "fragment.Interface",
        TypeFragmentKind::Class => "fragment.Class",
        TypeFragmentKind::Enum => "fragment.Enum",
    }
}

fn type_fragment_placement_name(placement: SourceTypeFragmentPlacement) -> &'static str {
    match placement {
        SourceTypeFragmentPlacement::Ordinary => "placement.Ordinary",
        SourceTypeFragmentPlacement::ClassExpression => "placement.ClassExpression",
        SourceTypeFragmentPlacement::DefaultClass => "placement.DefaultClass",
        SourceTypeFragmentPlacement::UnadmittedClassStaticBlock => {
            "placement.UnadmittedClassStaticBlock"
        }
        SourceTypeFragmentPlacement::UnadmittedAmbientExternalModule => {
            "placement.UnadmittedAmbientExternalModule"
        }
        SourceTypeFragmentPlacement::GlobalAugmentationBody { .. } => {
            "placement.GlobalAugmentationBody"
        }
    }
}

fn interface_occurrence_name(kind: SourceInterfaceOccurrenceKind) -> &'static str {
    match kind {
        SourceInterfaceOccurrenceKind::Header => "interface_occurrence.Header",
        SourceInterfaceOccurrenceKind::Heritage => "interface_occurrence.Heritage",
        SourceInterfaceOccurrenceKind::Member => "interface_occurrence.Member",
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("library crate is inside workspace/crates")
        .to_owned()
}

fn read_rust_tree(root: &Path) -> String {
    let mut entries = fs::read_dir(root)
        .expect("candidate source directory")
        .map(|entry| entry.expect("candidate source entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    let mut source = String::new();
    for path in entries {
        if path.is_dir() {
            source.push_str(&read_rust_tree(&path));
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            source.push_str(&fs::read_to_string(path).expect("candidate Rust source"));
        }
    }
    source
}

fn rust_source_paths(crate_root: &Path) -> Vec<String> {
    fn collect(crate_root: &Path, directory: &Path, paths: &mut Vec<String>) {
        let mut entries = fs::read_dir(directory)
            .expect("candidate source tree")
            .map(|entry| entry.expect("candidate source entry").path())
            .collect::<Vec<_>>();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                collect(crate_root, &path, paths);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
                paths.push(
                    path.strip_prefix(crate_root)
                        .expect("candidate-relative source path")
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }

    let mut paths = Vec::new();
    collect(crate_root, crate_root, &mut paths);
    paths
}

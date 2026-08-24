//! RED contract for WU3's eager project-local declaration universe.

use super::base::UserDeltaProjectInputForTest;
use super::compiler::LibraryCompilerWorkScopeForTest;
use super::provider::{
    CompleteSourceDeclarationOracleForTest, InitializationMeasurement, LibraryBaseProvider,
};
use crate::binder::declaration::SourceDeclarationManifestBuildScopeForTest;
use crate::binder::namespace::{
    DeclarationOwner, DeclarationSyntaxFacts, MergeDeclarationKind, MergeDisposition, NamespaceId,
    NamespaceInstanceState, NamespaceOwner, SourceUnitKey, VariableKind,
};
use crate::binder::scope::ScopeId;
use crate::check::checker::events::UserEventReservationScopeForTest;
use crate::check::checker::library_compiler::{
    CanonicalLibraryFrontendWorkScopeForTest, UserDeltaForkScopeForTest, UserSourceWorkScopeForTest,
};
use crate::check::query::QueryCacheWriteScopeForTest;
use crate::relate::cache::RelationCacheWriteScopeForTest;
use crate::types::layered::LocalRowAllocationScopeForTest;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

const ORACLE_DIGESTS: [&str; 10] = [
    "10cee1d655a8e48e07d76bd27c003a7f77b1da68418e907587cf3e2cec7b9554",
    "2f81982f885419b59bb651b4c0712e8969f5c3059cac0efb011a690e8a375502",
    "26fc664bc5c80c01ef976cbcf18c68dcdb39ee1972b6252a8931f38b58958074",
    "b25ad87fc6bf22f0d6f16dc8048513b0b69984160c7b8728846239a39f33b2b8",
    "bcef9a9c06a284dc1226d477dc0e700ea9e95960d818207336de1ab1029c1fda",
    "be0dcf6fe3631e608cdc27d6572d7327d46b19eaa9ca3776d8fdf10ad85be4d2",
    "6cdc5c4a23a7c6647bb21b514897a39eb0f7a7d9539bf7d2f1056f63a63af428",
    "1291935b4c0f8766c5416c46f5812b73775a3b81e354f323945909ab8ecf1890",
    "76eb292a3f08a9d306eb97915089037e66f014fb21029957949d5a5421678b23",
    "d63cd42099b24adf5c411fda472a5d9c1b66104aa354ef22ad750af54fbad1a6",
];

const PERTURB_LOCAL_ROW_DELTA: u64 = 17;

#[test]
fn complete_source_oracle_freezes_library_lexical_scope_product_inputs() {
    let projection =
        complete_source_oracle(&[input("/candidate/lexical_scope_oracle.ts", "export {};\n")])
            .projection;
    let callable_scopes = projection
        .scopes
        .iter()
        .filter(|row| row.contains("library:") && row.contains("|kind=Function|"))
        .cloned()
        .collect::<Vec<_>>();
    let declaration_scopes = projection
        .declarations
        .iter()
        .filter(|row| row.starts_with("library:"))
        .filter_map(|row| row.split_once("|v=").map(|(scope, _)| scope.to_owned()))
        .collect::<Vec<_>>();
    let scope_classes = declaration_scope_classes(&declaration_scopes);
    let parent_classes = callable_parent_classes(&callable_scopes);
    assert_eq!(callable_scopes.len(), 153);
    assert_eq!(
        rows_digest(&callable_scopes),
        "09bab36a21adae1235dd2cb33289fb588e668db0f78f3f403d86d45a26dfe351"
    );
    assert_eq!(declaration_scopes.len(), 13_958);
    assert_eq!(scope_classes, (10_228, 3_521, 209));
    assert_eq!(parent_classes, (153, 0));
    assert_eq!(
        rows_digest(&declaration_scopes),
        "25fcd4c7e7e521032582e145aa4b379246fd5f166f6362cf58a01d208ec6f04a"
    );
}

#[test]
fn declaration_product_retains_exact_library_lexical_scope_inputs() {
    let product = LibraryBaseProvider::new()
        .declaration_product()
        .expect("immutable declaration product");
    let compiler_work = LibraryCompilerWorkScopeForTest::start();
    let frontend_work = CanonicalLibraryFrontendWorkScopeForTest::start();
    let (callable_scopes, declaration_scopes) = product_lexical_scope_projection_for_test(&product)
        .expect("product exposes raw store-independent lexical scope facts");
    let compiler_work = compiler_work.finish();
    let frontend_work = frontend_work.finish();
    assert_eq!(
        (
            compiler_work.compiles,
            compiler_work.parses,
            compiler_work.binds,
            compiler_work.checks,
        ),
        (0, 0, 0, 0)
    );
    assert_eq!(
        (
            frontend_work.entries,
            frontend_work.parse_units,
            frontend_work.bind_batches,
            frontend_work.bind_units,
        ),
        (0, 0, 0, 0)
    );
    assert_eq!(callable_scopes.len(), 153);
    assert_eq!(
        rows_digest(&callable_scopes),
        "09bab36a21adae1235dd2cb33289fb588e668db0f78f3f403d86d45a26dfe351"
    );
    assert_eq!(declaration_scopes.len(), 13_958);
    assert_eq!(
        declaration_scope_classes(&declaration_scopes),
        (10_228, 3_521, 209)
    );
    assert_eq!(callable_parent_classes(&callable_scopes), (153, 0));
    assert_eq!(
        rows_digest(&declaration_scopes),
        "25fcd4c7e7e521032582e145aa4b379246fd5f166f6362cf58a01d208ec6f04a"
    );

    let schema = include_str!("../../typokat-library-product/src/schema.rs");
    let keys = include_str!("../../typokat-library-product/src/keys.rs");
    for required in [
        "pub struct CallableScopeRecipe {\n    pub site: SourceSite,\n    pub parent: LexicalScopeParent,\n}",
        "pub enum LexicalScopeParent {\n    Region(RegionKey),\n    Callable(CallableScopeKey),\n}",
        "pub enum DeclarationLexicalScope {\n    None,\n    Region(RegionKey),\n    Callable(CallableScopeKey),\n}",
        "pub lexical_scope: DeclarationLexicalScope",
        "callable_scopes: Box<[CallableScopeRecipe]>",
    ] {
        assert!(
            schema.contains(required),
            "missing product field: {required}"
        );
    }
    assert!(keys.contains("pub struct CallableScopeKey"));
    for forbidden in [
        "ScopeId",
        "SymbolId",
        "DeclId",
        "TypeGroupId",
        "ValueStorageId",
        "Binder",
        "Program<'",
        "Allocator",
    ] {
        assert!(
            !schema.contains(forbidden),
            "lexical scope product retained local identity: {forbidden}"
        );
    }
    let spec = include_str!("project_local_declaration_universe_spec.rs");
    let start = spec
        .rfind("fn product_lexical_scope_projection_for_test")
        .expect("raw product projection helper exists");
    let body = rust_function_body(spec, start);
    for forbidden in [
        "complete_source_oracle",
        "LibraryCompiler",
        "compile_library",
        "checkpoint",
        "FrozenLibraryBase",
    ] {
        assert!(
            !body.contains(forbidden),
            "product scope projection used an external oracle: {forbidden}"
        );
    }
    for required in [
        "product.callable_scopes()",
        "product.declarations()",
        "product.regions()",
        "product.sources()",
        "product.strings()",
        "for scope in product.callable_scopes()",
        "for declaration in product.declarations()",
    ] {
        assert!(
            body.contains(required),
            "product projection did not dereference raw field: {required}"
        );
    }

    let projector = include_str!("../../typokat-binder/src/binder/product.rs");
    let callable_start = projector
        .find("let mut callable_facts")
        .expect("callable projection exists");
    let callable_end = projector[callable_start..]
        .find("for occurrence in input.manifest.declarations() {\n            let Some(bound)")
        .map(|offset| callable_start + offset)
        .expect("declaration projection follows callable projection");
    let callable_projection = &projector[callable_start..callable_end];
    assert!(callable_projection.contains("lexical_regions.get(&parent_scope.0)"));
    assert!(!callable_projection.contains("region_ranges"));

    let declaration_start = projector
        .find("let lexical_scope = match bound.site.scope")
        .expect("declaration lexical projection exists");
    let declaration_end = projector[declaration_start..]
        .find("let payload = lower_declaration_payload")
        .map(|offset| declaration_start + offset)
        .expect("payload projection follows lexical scope projection");
    let declaration_projection = &projector[declaration_start..declaration_end];
    assert!(declaration_projection.contains("lexical_regions"));
    assert!(!declaration_projection.contains("region_ranges"));
}

fn product_lexical_scope_projection_for_test(
    product: &typokat_library_product::LibraryDeclarationProduct,
) -> Result<(Vec<String>, Vec<String>), &'static str> {
    let _strings = product.strings();
    let _sources = product.sources();
    let _regions = product.regions();
    let _declarations = product.declarations();
    let _callable_scopes = product.callable_scopes();
    fn source_name(
        product: &typokat_library_product::LibraryDeclarationProduct,
        source: typokat_library_product::SourceKey,
    ) -> Result<String, &'static str> {
        let source = product
            .sources()
            .get(source.index())
            .ok_or("invalid source")?;
        Ok(format!("library:{:02}", source.ordinal))
    }

    fn container_scope(
        product: &typokat_library_product::LibraryDeclarationProduct,
        container: typokat_library_product::ContainerKey,
    ) -> Result<String, &'static str> {
        let recipe = product
            .containers()
            .get(container.index())
            .ok_or("invalid container")?;
        match recipe.kind {
            typokat_library_product::ContainerKind::CompilationGlobal => {
                Ok("scope:compilation-global".to_owned())
            }
            typokat_library_product::ContainerKind::ExternalModule => {
                let anchor = recipe.anchor.ok_or("external module lacks anchor")?;
                Ok(format!(
                    "scope:module:{}",
                    source_name(product, anchor.source)?
                ))
            }
            typokat_library_product::ContainerKind::Namespace
            | typokat_library_product::ContainerKind::AmbientExternalModule => {
                let parent =
                    container_scope(product, recipe.parent.ok_or("namespace lacks parent")?)?;
                let name = product
                    .strings()
                    .get(recipe.name.ok_or("namespace lacks name")?.index())
                    .ok_or("invalid namespace name")?;
                Ok(format!("{parent}::{name}"))
            }
        }
    }

    fn region_scope(
        product: &typokat_library_product::LibraryDeclarationProduct,
        region: typokat_library_product::RegionKey,
    ) -> Result<String, &'static str> {
        let recipe = product
            .regions()
            .get(region.index())
            .ok_or("invalid region")?;
        let source = source_name(product, recipe.site.source)?;
        match recipe.kind {
            typokat_library_product::RegionKind::Source => {
                match product.sources()[recipe.site.source.index()].classification {
                    typokat_library_product::SourceClassification::Script => {
                        Ok("scope:compilation-global".to_owned())
                    }
                    typokat_library_product::SourceClassification::ExternalModule => {
                        Ok(format!("scope:module:{source}"))
                    }
                }
            }
            typokat_library_product::RegionKind::Namespace
            | typokat_library_product::RegionKind::AmbientExternalModule => {
                let fragment_start = product
                    .declarations()
                    .iter()
                    .find_map(|declaration| match declaration.payload {
                        typokat_library_product::DeclarationPayload::Namespace { child_region }
                            if child_region == region =>
                        {
                            Some(declaration.site.start)
                        }
                        typokat_library_product::DeclarationPayload::Variable { .. }
                        | typokat_library_product::DeclarationPayload::Function { .. }
                        | typokat_library_product::DeclarationPayload::Class { .. }
                        | typokat_library_product::DeclarationPayload::Interface { .. }
                        | typokat_library_product::DeclarationPayload::TypeAlias { .. }
                        | typokat_library_product::DeclarationPayload::Enum { .. }
                        | typokat_library_product::DeclarationPayload::Namespace { .. }
                        | typokat_library_product::DeclarationPayload::Import
                        | typokat_library_product::DeclarationPayload::ImportEquals
                        | typokat_library_product::DeclarationPayload::NamespaceExport
                        | typokat_library_product::DeclarationPayload::Global { .. }
                        | typokat_library_product::DeclarationPayload::Parameter { .. }
                        | typokat_library_product::DeclarationPayload::CatchParameter { .. }
                        | typokat_library_product::DeclarationPayload::DefaultExportExpression => {
                            None
                        }
                    })
                    .ok_or("namespace region lacks declaration")?;
                Ok(format!(
                    "scope:namespace-private:{}:{source}:{fragment_start}",
                    container_scope(product, recipe.container)?
                ))
            }
            typokat_library_product::RegionKind::GlobalAugmentation => {
                Ok("scope:compilation-global".to_owned())
            }
        }
    }

    fn callable_scope(
        product: &typokat_library_product::LibraryDeclarationProduct,
        key: typokat_library_product::CallableScopeKey,
    ) -> Result<String, &'static str> {
        let scope = product
            .callable_scopes()
            .get(key.index())
            .ok_or("invalid callable scope")?;
        Ok(format!(
            "scope:function:scope:module:{}:{}",
            source_name(product, scope.site.source)?,
            scope.site.start
        ))
    }

    fn declaration_kind(payload: &typokat_library_product::DeclarationPayload) -> &'static str {
        match payload {
            typokat_library_product::DeclarationPayload::Variable { .. } => "Variable",
            typokat_library_product::DeclarationPayload::Function { .. } => "Function",
            typokat_library_product::DeclarationPayload::Class { .. } => "Class",
            typokat_library_product::DeclarationPayload::Interface { .. } => "Interface",
            typokat_library_product::DeclarationPayload::TypeAlias { .. } => "TypeAlias",
            typokat_library_product::DeclarationPayload::Enum { .. } => "Enum",
            typokat_library_product::DeclarationPayload::Namespace { .. } => "Namespace",
            typokat_library_product::DeclarationPayload::Import => "Import",
            typokat_library_product::DeclarationPayload::ImportEquals => "ImportEquals",
            typokat_library_product::DeclarationPayload::NamespaceExport => "NamespaceExport",
            typokat_library_product::DeclarationPayload::Global { .. } => "Global",
            typokat_library_product::DeclarationPayload::Parameter { .. } => "Parameter",
            typokat_library_product::DeclarationPayload::CatchParameter { .. } => "CatchParameter",
            typokat_library_product::DeclarationPayload::DefaultExportExpression => {
                "DefaultExportExpression"
            }
        }
    }

    let mut callable_scopes = Vec::new();
    for scope in product.callable_scopes() {
        let parent = match scope.parent {
            typokat_library_product::LexicalScopeParent::Region(region) => {
                region_scope(product, region)?
            }
            typokat_library_product::LexicalScopeParent::Callable(callable) => {
                callable_scope(product, callable)?
            }
        };
        let callable = format!(
            "scope:function:scope:module:{}:{}",
            source_name(product, scope.site.source)?,
            scope.site.start
        );
        callable_scopes.push(format!(
            "{callable}|kind=Function|parent={parent}|public=none"
        ));
    }
    callable_scopes.sort();

    let mut declaration_scopes = Vec::new();
    for declaration in product.declarations() {
        let source = source_name(product, declaration.site.source)?;
        let scope = match declaration.lexical_scope {
            typokat_library_product::DeclarationLexicalScope::None => "none".to_owned(),
            typokat_library_product::DeclarationLexicalScope::Region(region) => {
                region_scope(product, region)?
            }
            typokat_library_product::DeclarationLexicalScope::Callable(callable) => {
                callable_scope(product, callable)?
            }
        };
        declaration_scopes.push(format!(
            "{source}|{}|{}:{}|{}:{}|scope={scope}",
            declaration_kind(&declaration.payload),
            declaration.site.start,
            declaration.site.end,
            declaration.binding_site.start,
            declaration.binding_site.end
        ));
    }
    declaration_scopes.sort();
    Ok((callable_scopes, declaration_scopes))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProductLexicalScopeCorruptionReceipt {
    validation_errors: Vec<String>,
}

#[test]
fn declaration_product_lexical_scope_corruption_controls_are_live() {
    let product = LibraryBaseProvider::new()
        .declaration_product()
        .expect("immutable declaration product");
    let compiler_work = LibraryCompilerWorkScopeForTest::start();
    let frontend_work = CanonicalLibraryFrontendWorkScopeForTest::start();
    let ordinary = product_lexical_scope_projection_for_test(&product)
        .expect("ordinary raw product scope projection");
    let changed = product_with_changed_callable_site_for_test(&product)
        .expect("in-bounds callable-site falsifier");
    changed
        .validate()
        .expect("in-bounds callable-site falsifier remains structurally valid");
    assert_eq!(changed.profile_identity(), product.profile_identity());
    assert_eq!(changed.strings(), product.strings());
    assert_eq!(changed.sources(), product.sources());
    assert_eq!(changed.containers(), product.containers());
    assert_eq!(changed.regions(), product.regions());
    assert_eq!(changed.declarations(), product.declarations());
    assert_eq!(changed.groups(), product.groups());
    assert_eq!(changed.type_parameters(), product.type_parameters());
    assert_eq!(changed.types(), product.types());
    assert_eq!(changed.members(), product.members());
    assert_eq!(changed.dependency_rows(), product.dependency_rows());
    assert_eq!(
        changed.namespace_attachment_rows(),
        product.namespace_attachment_rows()
    );
    assert_eq!(
        changed.value_attachment_rows(),
        product.value_attachment_rows()
    );
    let changed = product_lexical_scope_projection_for_test(&changed)
        .expect("changed raw product scope projection");
    assert_eq!(ordinary.0.len(), changed.0.len());
    assert_eq!(ordinary.1, changed.1);
    let ordinary_scopes = ordinary.0.iter().collect::<BTreeSet<_>>();
    let changed_scopes = changed.0.iter().collect::<BTreeSet<_>>();
    assert_eq!(
        ordinary_scopes
            .symmetric_difference(&changed_scopes)
            .count(),
        2
    );

    let receipt = product_lexical_scope_corruption_receipt_for_test(&product)
        .expect("five real product mutations reach typed validator failures");
    assert_eq!(
        receipt.validation_errors,
        [
            "CallableScopeSourceOutOfBounds",
            "CallableScopeParentOutOfBounds",
            "CallableScopeParentCycle",
            "DeclarationLexicalScopeOutOfBounds",
            "DeclarationLexicalScopeRegionMismatch",
        ]
    );
    let mut distinct = receipt.validation_errors.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(distinct.len(), 5);
    let compiler_work = compiler_work.finish();
    let frontend_work = frontend_work.finish();
    assert_eq!(
        (
            compiler_work.compiles,
            compiler_work.parses,
            compiler_work.binds,
            compiler_work.checks,
        ),
        (0, 0, 0, 0)
    );
    assert_eq!(
        (
            frontend_work.entries,
            frontend_work.parse_units,
            frontend_work.bind_batches,
            frontend_work.bind_units,
        ),
        (0, 0, 0, 0)
    );

    let validation = include_str!("../../typokat-library-product/src/validate.rs");
    let start = validation
        .find("pub fn lexical_scope_corruption_receipt_for_test")
        .expect("product-owned corruption helper exists");
    let body = rust_function_body(validation, start);
    for required in [
        "callable_scopes[",
        ".site",
        ".parent",
        "declarations[",
        ".lexical_scope",
        ".validate()",
        "CallableScopeSourceOutOfBounds",
        "CallableScopeParentOutOfBounds",
        "CallableScopeParentCycle",
        "DeclarationLexicalScopeOutOfBounds",
        "DeclarationLexicalScopeRegionMismatch",
    ] {
        assert!(
            body.contains(required),
            "corruption helper lacks live mutation/validation evidence: {required}"
        );
    }

    let spec = include_str!("project_local_declaration_universe_spec.rs");
    for (wrapper, required) in [
        (
            "fn product_with_changed_callable_site_for_test",
            "typokat_library_product::with_changed_callable_site_for_test(product)",
        ),
        (
            "fn product_lexical_scope_corruption_receipt_for_test",
            "typokat_library_product::lexical_scope_corruption_receipt_for_test(product)",
        ),
    ] {
        let start = spec.rfind(wrapper).expect("spec wrapper exists");
        let body = rust_function_body(spec, start);
        assert!(
            body.contains(required),
            "wrapper bypassed product API: {wrapper}"
        );
        for forbidden in [
            "complete_source_oracle",
            "LibraryCompiler",
            "checkpoint",
            "FrozenLibraryBase",
            "build_packaged_declaration_product",
        ] {
            assert!(
                !body.contains(forbidden),
                "wrapper used external construction path: {forbidden}"
            );
        }
    }
}

fn product_with_changed_callable_site_for_test(
    product: &typokat_library_product::LibraryDeclarationProduct,
) -> Result<typokat_library_product::LibraryDeclarationProduct, &'static str> {
    typokat_library_product::with_changed_callable_site_for_test(product)
}

fn product_lexical_scope_corruption_receipt_for_test(
    product: &typokat_library_product::LibraryDeclarationProduct,
) -> Result<ProductLexicalScopeCorruptionReceipt, &'static str> {
    let receipt = typokat_library_product::lexical_scope_corruption_receipt_for_test(product)?;
    Ok(ProductLexicalScopeCorruptionReceipt {
        validation_errors: receipt
            .validation_errors
            .into_iter()
            .map(|error| format!("{error:?}"))
            .collect(),
    })
}

fn rust_function_body(source: &str, start: usize) -> &str {
    let open = source[start..]
        .find('{')
        .map(|offset| start + offset)
        .expect("function body opens");
    let mut depth = 0usize;
    for (offset, byte) in source.as_bytes()[open..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.checked_sub(1).expect("balanced function braces");
                if depth == 0 {
                    return &source[start..=open + offset];
                }
            }
            _ => {}
        }
    }
    panic!("function body closes")
}

fn declaration_scope_classes(rows: &[String]) -> (usize, usize, usize) {
    let mut none = 0;
    let mut region = 0;
    let mut callable = 0;
    for row in rows {
        let scope = row
            .split_once("|scope=")
            .map(|(_, suffix)| suffix.split('|').next().unwrap_or(suffix))
            .expect("declaration row carries lexical scope");
        if scope == "none" {
            none += 1;
        } else if scope.starts_with("scope:function:") {
            callable += 1;
        } else if is_region_scope(scope) {
            region += 1;
        } else {
            panic!("unclassified declaration lexical scope: {scope}");
        }
    }
    (none, region, callable)
}

fn callable_parent_classes(rows: &[String]) -> (usize, usize) {
    let mut region = 0;
    let mut callable = 0;
    for row in rows {
        let parent = row
            .split_once("|parent=")
            .map(|(_, suffix)| suffix.split('|').next().unwrap_or(suffix))
            .expect("callable scope row carries parent");
        if parent.starts_with("scope:function:") {
            callable += 1;
        } else if is_region_scope(parent) {
            region += 1;
        } else {
            panic!("unclassified callable parent scope: {parent}");
        }
    }
    (region, callable)
}

fn is_region_scope(scope: &str) -> bool {
    matches!(
        scope,
        "scope:prelude" | "scope:compilation-global" | "scope:script-namespace-root"
    ) || [
        "scope:module:library:",
        "scope:namespace-public:",
        "scope:namespace-private:",
        "scope:global-overlay:",
    ]
    .iter()
    .any(|prefix| scope.starts_with(prefix))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct DeclarationProjection {
    scopes: Vec<String>,
    declarations: Vec<String>,
    groups: Vec<String>,
    namespaces: Vec<String>,
    attachments: Vec<String>,
    augmentations: Vec<String>,
    global_contributors: Vec<String>,
    modules: Vec<String>,
}

impl DeclarationProjection {
    fn digest(&self) -> String {
        let mut text = String::new();
        for (kind, rows) in [
            ("scope", &self.scopes),
            ("declaration", &self.declarations),
            ("group", &self.groups),
            ("namespace", &self.namespaces),
            ("attachment", &self.attachments),
            ("augmentation", &self.augmentations),
            ("global", &self.global_contributors),
            ("module", &self.modules),
        ] {
            for row in rows {
                text.push_str(kind);
                text.push('|');
                text.push_str(row);
                text.push('\n');
            }
        }
        format!("{:x}", Sha256::digest(text.as_bytes()))
    }
}

struct OracleReceipt {
    paths: Vec<String>,
    parses: u64,
    binds: u64,
    checks: u64,
    manifests: u64,
    projection: DeclarationProjection,
}

#[test]
fn real_complete_source_oracle_is_binder_only_and_covers_every_candidate_project() {
    let cases = project_cases();
    assert_eq!(cases.len(), ORACLE_DIGESTS.len());
    for (index, inputs) in cases.into_iter().enumerate() {
        let receipt = complete_source_oracle(&inputs);
        let mut paths = inputs
            .iter()
            .map(|input| input.path.to_owned())
            .collect::<Vec<_>>();
        paths.sort();
        assert_eq!(receipt.paths, paths);
        let input_count = u64::try_from(inputs.len()).expect("project input count fits u64");
        assert_eq!(receipt.parses, input_count);
        assert_eq!(receipt.binds, input_count);
        assert_eq!(receipt.checks, 0);
        assert_eq!(receipt.manifests, input_count + 83);
        assert!(!receipt.projection.scopes.is_empty());
        assert!(!receipt.projection.declarations.is_empty());
        assert!(!receipt.projection.groups.is_empty());
        assert!(!receipt.projection.modules.is_empty());
        assert_eq!(receipt.projection.digest(), ORACLE_DIGESTS[index]);
    }
}

#[test]
fn real_oracle_proves_order_normalization_and_project_isolation() {
    let cases = project_cases();
    let isolation_a = complete_source_oracle(&cases[2]).projection;
    let isolation_b = complete_source_oracle(&cases[3]).projection;
    assert_ne!(isolation_a.digest(), isolation_b.digest());

    for inputs in cases.into_iter().filter(|inputs| inputs.len() > 1) {
        let forward = complete_source_oracle(&inputs).projection;
        let mut reversed_inputs = inputs;
        reversed_inputs.reverse();
        let reverse = complete_source_oracle(&reversed_inputs).projection;
        for (kind, left, right) in [
            ("scopes", &forward.scopes, &reverse.scopes),
            ("declarations", &forward.declarations, &reverse.declarations),
            ("groups", &forward.groups, &reverse.groups),
            ("namespaces", &forward.namespaces, &reverse.namespaces),
            ("attachments", &forward.attachments, &reverse.attachments),
            (
                "augmentations",
                &forward.augmentations,
                &reverse.augmentations,
            ),
            (
                "global_contributors",
                &forward.global_contributors,
                &reverse.global_contributors,
            ),
            ("modules", &forward.modules, &reverse.modules),
        ] {
            assert!(
                left == right,
                "reverse input changed {kind}: {} != {}",
                rows_digest(left),
                rows_digest(right)
            );
        }
    }
}

fn rows_digest(rows: &[String]) -> String {
    format!("{:x}", Sha256::digest(rows.join("\n").as_bytes()))
}

#[test]
fn changed_raw_source_defeats_a_fixed_expected_receipt() {
    let original = vec![input(
        "/candidate/changed.ts",
        "interface WU3ChangedInput { original: number }\n",
    )];
    let changed = vec![input(
        "/candidate/changed.ts",
        "interface WU3ChangedInput { changed: string }\n",
    )];
    let original = complete_source_oracle(&original).projection;
    let changed = complete_source_oracle(&changed).projection;
    assert_ne!(original.digest(), changed.digest());
    assert_ne!(original.declarations, changed.declarations);
}

#[test]
fn malformed_user_source_is_rejected_before_binding_or_semantics() {
    let provider = LibraryBaseProvider::new();
    let source_work = UserSourceWorkScopeForTest::start();
    let manifest_work = SourceDeclarationManifestBuildScopeForTest::start();
    let inputs = vec![input("/candidate/malformed.ts", "const broken = {")];
    let result = provider.complete_source_declaration_oracle_for_test(&inputs);
    let manifests = manifest_work.finish();
    let work = source_work.finish();

    assert!(result.is_err());
    assert_eq!(work.checks, 0);
    assert_eq!(manifests, 83);
    assert_eq!(
        provider.measurement_for_test(),
        InitializationMeasurement {
            attempts: 0,
            publications: 0,
        }
    );
    assert!(!provider.declaration_product_initialized_for_test());
}

#[test]
fn oracle_does_not_initialize_the_product_or_the_frozen_semantic_base() {
    let provider = LibraryBaseProvider::new();
    let inputs = vec![input("/candidate/empty.ts", "export {};\n")];
    provider
        .complete_source_declaration_oracle_for_test(&inputs)
        .expect("binder-only oracle");
    assert_eq!(
        provider.measurement_for_test(),
        InitializationMeasurement {
            attempts: 0,
            publications: 0,
        }
    );
    assert!(!provider.declaration_product_initialized_for_test());

    let source = include_str!("provider.rs");
    let start = source
        .find("pub(super) fn complete_source_declaration_oracle_for_test")
        .expect("oracle entry exists");
    let end = source[start..]
        .find("pub(super) fn continue_library_binder_checkpoint_for_test")
        .map(|offset| start + offset)
        .expect("next provider entry exists");
    let body = &source[start..end];
    for forbidden in [
        "self.initialized",
        "FrozenLibraryBase",
        "collision",
        "replay",
        "compile_owned",
        "publish",
        "Interner",
        "Store",
        "TypeId",
        "EventStore",
        "cache",
    ] {
        assert!(
            !body.contains(forbidden),
            "forbidden oracle entry: {forbidden}"
        );
    }
}

#[test]
#[ignore = "WU3 RED: the complete-source oracle still crosses checkpoint continuations"]
fn complete_source_oracle_uses_one_fresh_project_root() {
    let projection =
        complete_source_oracle(&[input("/candidate/one_root_oracle.ts", "export {};\n")])
            .projection;

    assert_eq!(
        projection
            .scopes
            .iter()
            .filter(|row| row.contains("kind=CompilationGlobal"))
            .count(),
        1,
        "one fresh project binder has one compilation-global root"
    );
    assert_eq!(
        projection
            .scopes
            .iter()
            .filter(|row| row.contains("kind=ScriptNamespaceRoot"))
            .count(),
        1,
        "one fresh project binder has one script-namespace root"
    );
    assert!(projection.scopes.iter().all(|row| {
        !row.contains("scope:unanchored:CompilationGlobal")
            && !row.contains("scope:unanchored:ScriptNamespaceRoot")
    }));

    let source = include_str!("provider.rs");
    let start = source
        .find("pub(super) fn complete_source_declaration_oracle_for_test")
        .expect("oracle entry exists");
    let body = rust_function_body(source, start);
    assert!(body.contains("compile_project_declaration_oracle_one_root_for_test"));
    for forbidden in [
        "compile_binder_checkpoint",
        "continue_library_project_binder_clean_for_test",
        "into_continuation",
    ] {
        assert!(
            !body.contains(forbidden),
            "one-root oracle crosses a forbidden lifecycle boundary: {forbidden}"
        );
    }
}

#[test]
#[ignore = "RED until PreparedProject instantiates the product into a fresh project binder"]
fn project_local_candidate_matches_real_oracle_under_id_perturbation() {
    for inputs in project_cases() {
        let oracle = complete_source_oracle(&inputs).projection;
        let provider = LibraryBaseProvider::new();
        let product = provider
            .declaration_product()
            .expect("immutable declaration product");
        assert_eq!(product.source_count(), 82);
        let source_work = UserSourceWorkScopeForTest::start();
        let manifest_work = SourceDeclarationManifestBuildScopeForTest::start();
        let library_frontend_work = CanonicalLibraryFrontendWorkScopeForTest::start();
        let compiler_work = LibraryCompilerWorkScopeForTest::start();
        let delta_work = UserDeltaForkScopeForTest::start();
        let events = UserEventReservationScopeForTest::start();
        let queries = QueryCacheWriteScopeForTest::start();
        let relations = RelationCacheWriteScopeForTest::start();
        let ordinary_rows = LocalRowAllocationScopeForTest::start();
        let ordinary = build_candidate_for_test(&provider, &inputs, false);
        let ordinary_rows = ordinary_rows.finish();
        let ordinary = ordinary.expect("WU3 candidate constructs a declaration-only universe");
        let perturbed_rows = LocalRowAllocationScopeForTest::start();
        let perturbed = build_candidate_for_test(&provider, &inputs, true);
        let perturbed_rows = perturbed_rows.finish();
        let perturbed = perturbed.expect("WU3 candidate survives unrelated local-id allocations");
        let reversed = if inputs.len() > 1 {
            let mut reversed_inputs = inputs.clone();
            reversed_inputs.reverse();
            let reversed_rows = LocalRowAllocationScopeForTest::start();
            let reversed = build_candidate_for_test(&provider, &reversed_inputs, false);
            let reversed_rows = reversed_rows.finish();
            Some((
                reversed.expect("WU3 candidate normalizes reverse input order"),
                reversed_rows,
            ))
        } else {
            None
        };
        let library_frontend_work = library_frontend_work.finish();
        let compiler_work = compiler_work.finish();
        let queries = queries.finish();
        let manifests = manifest_work.finish();
        let work = source_work.finish();
        let input_count = u64::try_from(inputs.len()).expect("project input count fits u64");

        assert!(Arc::ptr_eq(&product, &ordinary.product));
        assert!(Arc::ptr_eq(&product, &perturbed.product));
        assert_ne!(ordinary_rows, 0);
        assert_eq!(
            perturbed_rows,
            ordinary_rows
                .checked_add(PERTURB_LOCAL_ROW_DELTA)
                .expect("fresh-binder row receipt fits u64")
        );
        assert_eq!(normalize_candidate(&ordinary), oracle);
        assert_eq!(normalize_candidate(&perturbed), oracle);
        assert_ne!(
            ordinary.bound.binder.checkpoint_ends(),
            perturbed.bound.binder.checkpoint_ends()
        );
        if let Some((reversed, reversed_rows)) = &reversed {
            assert!(Arc::ptr_eq(&product, &reversed.product));
            assert_eq!(*reversed_rows, ordinary_rows);
            assert_eq!(normalize_candidate(reversed), oracle);
        }
        let candidate_runs = if reversed.is_some() { 3 } else { 2 };
        assert_eq!(
            (work.parses, work.binds, work.checks),
            (
                candidate_runs * input_count,
                candidate_runs * input_count,
                0
            )
        );
        assert_eq!(manifests, candidate_runs * input_count);
        assert_eq!(library_frontend_work.entries, 0);
        assert_eq!(library_frontend_work.parse_units, 0);
        assert_eq!(library_frontend_work.bind_batches, 0);
        assert_eq!(library_frontend_work.bind_units, 0);
        assert_eq!(library_frontend_work.full_source_products_consumed, 0);
        assert_eq!(library_frontend_work.checkpoint_products_consumed, 0);
        assert_eq!(
            (
                compiler_work.compiles,
                compiler_work.parses,
                compiler_work.binds,
                compiler_work.checks,
            ),
            (0, 0, 0, 0)
        );
        assert_eq!(delta_work.finish(), 0);
        assert_eq!(events.finish(), 0);
        assert_eq!((queries.projection, queries.evaluator), (0, 0));
        assert_eq!(relations.finish(), 0);
        assert!(Arc::ptr_eq(
            &product,
            &provider
                .declaration_product()
                .expect("provider reuses immutable declaration product")
        ));
        assert_eq!(
            provider.measurement_for_test(),
            InitializationMeasurement {
                attempts: 0,
                publications: 0,
            }
        );
    }

    assert_candidate_implementation_boundary();
}

#[test]
#[ignore = "RED until project-local declaration universes are isolated per construction"]
fn candidate_batch_order_does_not_leak_augmentations_or_identities() {
    let cases = project_cases();
    let a = &cases[2];
    let b = &cases[3];
    let forward_provider = LibraryBaseProvider::new();
    let a_forward = build_candidate_for_test(&forward_provider, a, false)
        .expect("first project in forward batch");
    let b_forward = build_candidate_for_test(&forward_provider, b, false)
        .expect("second project in forward batch");
    let reverse_provider = LibraryBaseProvider::new();
    let b_reverse = build_candidate_for_test(&reverse_provider, b, false)
        .expect("first project in reverse batch");
    let a_reverse = build_candidate_for_test(&reverse_provider, a, false)
        .expect("second project in reverse batch");

    let a_forward_after_b = normalize_candidate(&a_forward);
    let b_forward_after_a = normalize_candidate(&b_forward);
    let b_reverse_after_a = normalize_candidate(&b_reverse);
    let a_reverse_after_b = normalize_candidate(&a_reverse);
    assert_eq!(a_forward_after_b, a_reverse_after_b);
    assert_eq!(b_forward_after_a, b_reverse_after_a);
    assert_ne!(a_forward_after_b, b_forward_after_a);
    assert_eq!(a_forward_after_b, complete_source_oracle(a).projection);
    assert_eq!(b_forward_after_a, complete_source_oracle(b).projection);
    assert!(Arc::ptr_eq(&a_forward.product, &b_forward.product));
    assert!(Arc::ptr_eq(&b_reverse.product, &a_reverse.product));
}

#[test]
#[ignore = "RED until malformed prepared projects are rejected before binder construction"]
fn candidate_rejects_malformed_input_without_semantic_work() {
    let provider = LibraryBaseProvider::new();
    let _product = provider
        .declaration_product()
        .expect("immutable declaration product");
    let source_work = UserSourceWorkScopeForTest::start();
    let manifests = SourceDeclarationManifestBuildScopeForTest::start();
    let local_rows = LocalRowAllocationScopeForTest::start();
    let events = UserEventReservationScopeForTest::start();
    let queries = QueryCacheWriteScopeForTest::start();
    let relations = RelationCacheWriteScopeForTest::start();
    let result = build_candidate_for_test(
        &provider,
        &[input("/candidate/malformed.ts", "const broken = {")],
        false,
    );
    let source_work = source_work.finish();
    let queries = queries.finish();

    assert!(result.is_err());
    assert_eq!((source_work.binds, source_work.checks), (0, 0));
    assert_eq!(manifests.finish(), 0);
    assert_eq!(local_rows.finish(), 0);
    assert_eq!(events.finish(), 0);
    assert_eq!((queries.projection, queries.evaluator), (0, 0));
    assert_eq!(relations.finish(), 0);
}

struct CandidateReceipt {
    product: Arc<typokat_library_product::LibraryDeclarationProduct>,
    library_units: Vec<crate::binder::bind::LibraryBinderUnit>,
    bound: crate::check::checker::BoundProjectBinder,
}

fn build_candidate_for_test(
    _provider: &LibraryBaseProvider,
    _inputs: &[UserDeltaProjectInputForTest<'_>],
    _perturb_local_ids: bool,
) -> Result<CandidateReceipt, &'static str> {
    Err("project-local declaration universe is not implemented")
}

fn normalize_candidate(candidate: &CandidateReceipt) -> DeclarationProjection {
    normalize_parts(&candidate.library_units, &candidate.bound)
}

fn assert_candidate_implementation_boundary() {
    let source = include_str!("provider.rs");
    let start = source
        .find("pub(super) fn prepare_project_declaration_universe_for_test")
        .expect("candidate provider entry exists");
    let end = source[start..]
        .find("\n    pub(")
        .map(|offset| start + offset)
        .unwrap_or(source.len());
    let body = &source[start..end];
    for forbidden in [
        "complete_source_declaration_oracle_for_test",
        "LibraryCompiler",
        "compile_binder_checkpoint",
        "FrozenLibraryBase",
        "checkpoint",
        "collision",
        "replay",
        "sparse",
        "Interner",
        "Store",
        "TypeId",
        "TypeParamId",
        "DeclaredRecipeId",
        "EventStore",
        "RelationCache",
        "QueryCache",
        "global_object",
    ] {
        assert!(
            !body.contains(forbidden),
            "forbidden candidate dependency: {forbidden}"
        );
    }
}

fn complete_source_oracle(inputs: &[UserDeltaProjectInputForTest<'_>]) -> OracleReceipt {
    let provider = LibraryBaseProvider::new();
    let source_work = UserSourceWorkScopeForTest::start();
    let manifest_work = SourceDeclarationManifestBuildScopeForTest::start();
    let oracle = provider
        .complete_source_declaration_oracle_for_test(inputs)
        .expect("binder-only complete-source oracle");
    let manifests = manifest_work.finish();
    let work = source_work.finish();
    let paths = oracle
        .bound
        .project_sources
        .iter()
        .map(|row| row.normalized_path.clone())
        .collect();
    let projection = normalize(&oracle);
    OracleReceipt {
        paths,
        parses: work.parses,
        binds: work.binds,
        checks: work.checks,
        manifests,
        projection,
    }
}

fn normalize(oracle: &CompleteSourceDeclarationOracleForTest) -> DeclarationProjection {
    normalize_parts(&oracle.library_units, &oracle.bound)
}

fn normalize_parts(
    library_units: &[crate::binder::bind::LibraryBinderUnit],
    bound: &crate::check::checker::BoundProjectBinder,
) -> DeclarationProjection {
    let binder = &bound.binder;
    let mut source_names = library_units
        .iter()
        .map(|unit| (unit.source, format!("library:{:02}", unit.ordinal.index())))
        .collect::<BTreeMap<_, _>>();
    source_names.extend(
        bound
            .project_sources
            .iter()
            .map(|row| (row.source, format!("user:{}", row.normalized_path))),
    );
    let module_names = library_units
        .iter()
        .map(|unit| (unit.module.0, source_names[&unit.source].clone()))
        .chain(
            bound
                .project_sources
                .iter()
                .map(|row| (row.module.0, source_names[&row.source].clone())),
        )
        .collect();
    let context = ProjectionContext {
        binder,
        source_names,
        module_names,
    };

    let mut scopes = (0..binder.graph.len())
        .filter_map(|index| {
            let id = ScopeId(u32::try_from(index).ok()?);
            let row = binder.graph.get(id)?;
            Some(format!(
                "{}|kind={:?}|parent={}|public={}",
                context.scope(id),
                row.kind,
                row.parent
                    .map(|parent| context.scope(parent))
                    .unwrap_or_else(|| "none".to_owned()),
                row.namespace_public
                    .map(|public| context.scope(public))
                    .unwrap_or_else(|| "none".to_owned())
            ))
        })
        .collect::<Vec<_>>();
    scopes.sort();

    let mut declarations = binder
        .declarations
        .iter()
        .map(|declaration| {
            let source = binder
                .source_for_module(declaration.site.module)
                .map(|source| context.source(source))
                .unwrap_or_else(|| "prelude".to_owned());
            let scope = declaration
                .site
                .scope
                .map(|scope| context.scope(scope))
                .unwrap_or_else(|| "none".to_owned());
            format!(
                "{source}|{:?}|{}:{}|{}:{}|scope={scope}|v={}|t={}|n={}",
                declaration.kind,
                declaration.site.declaration_span.start,
                declaration.site.declaration_span.end,
                declaration.site.binding_span.start,
                declaration.site.binding_span.end,
                declaration
                    .value_storage
                    .map(|storage| context.value_storage(storage))
                    .unwrap_or_else(|| "none".to_owned()),
                declaration
                    .type_group
                    .map(|group| context.type_group(group))
                    .unwrap_or_else(|| "none".to_owned()),
                declaration
                    .namespace
                    .map(|namespace| context.namespace(namespace))
                    .unwrap_or_else(|| "none".to_owned())
            )
        })
        .collect::<Vec<_>>();
    declarations.sort();

    let mut groups = binder
        .type_groups
        .iter()
        .map(|group| {
            let fragments = group
                .fragments
                .iter()
                .map(|fragment| {
                    format!(
                        "{}:{}:{:?}:{}",
                        context.source(fragment.source),
                        fragment.site.binding_span.start,
                        fragment.kind,
                        context.scope(fragment.scope)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            let containers = group
                .fragments
                .iter()
                .map(|fragment| context.scope(fragment.scope))
                .collect::<BTreeSet<_>>();
            let mut slots = BTreeSet::new();
            for fragment in &group.fragments {
                if let Some(symbol) = binder
                    .graph
                    .resolve(fragment.scope, &group.name)
                    .and_then(|symbol| binder.symbols.get(symbol))
                {
                    if let Some(storage) = symbol.value {
                        slots.insert(format!("value={}", context.value_storage(storage)));
                    }
                    if let Some(group) = symbol.ty {
                        slots.insert(format!("type={}", context.type_group(group)));
                    }
                    if let Some(namespace) = symbol.ns {
                        slots.insert(format!("namespace={}", context.namespace(namespace)));
                    }
                }
            }
            format!(
                "{}|containers={containers:?}|slots={slots:?}|{fragments}",
                group.name
            )
        })
        .collect::<Vec<_>>();
    groups.sort();

    let mut namespaces = binder
        .namespaces
        .namespaces()
        .map(|namespace| {
            let fragments = namespace
                .fragments
                .iter()
                .filter_map(|id| binder.namespaces.fragment(*id))
                .map(|fragment| {
                    let members = fragment
                        .members
                        .iter()
                        .filter_map(|member| binder.namespaces.member(*member))
                        .map(|member| {
                            format!(
                                "{}:{}:{:?}:{:?}:{:?}:target={}:decl={}:symbol={}",
                                context.source(member.source),
                                member.binding_span.start,
                                member.name,
                                member.spaces,
                                member.kind,
                                context.declaration_owner(member.target),
                                member
                                    .declaration
                                    .map(|id| context.declaration(id))
                                    .unwrap_or_else(|| "none".to_owned()),
                                member
                                    .symbol
                                    .map(|id| context.symbol(id))
                                    .unwrap_or_else(|| "none".to_owned())
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("+");
                    format!(
                        "{}:{}:parent={}:private={}:public={}:members={members}",
                        context.source(fragment.source),
                        fragment.source_start,
                        context.scope(fragment.lexical_parent),
                        context.scope(fragment.private_scope),
                        context.scope(fragment.public_scope)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{}|owner={}|public={}|fragments={fragments}|standalone={}",
                namespace.name,
                context.namespace_owner(namespace.owner),
                context.scope(namespace.public_scope),
                binder
                    .namespaces
                    .standalone_value_storage(namespace.id)
                    .map(|storage| context.value_storage(storage))
                    .unwrap_or_else(|| "none".to_owned())
            )
        })
        .collect::<Vec<_>>();
    namespaces.sort();

    let mut attachments = binder
        .namespaces
        .merges()
        .filter_map(|merge| {
            binder
                .namespace_value_attachment_for_owner(merge.owner, &merge.name)
                .map(|attachment| {
                    let fragments = attachment
                        .fragments
                        .iter()
                        .map(|fragment| {
                            format!(
                                "{}:{}:{}",
                                context.namespace(fragment.namespace),
                                context.source(fragment.source),
                                fragment.source_start
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    let members = attachment
                        .members
                        .iter()
                        .map(|member| {
                            format!(
                                "{}:{}:{}:decl={}:value={}:symbol={}:kind={:?}:publication={:?}",
                                context.source(member.source),
                                member.site.binding_span.start,
                                member.name,
                                context.declaration(member.declaration),
                                member
                                    .value_storage
                                    .map(|storage| context.value_storage(storage))
                                    .unwrap_or_else(|| "none".to_owned()),
                                member
                                    .symbol
                                    .map(|symbol| context.symbol(symbol))
                                    .unwrap_or_else(|| "none".to_owned()),
                                member.kind,
                                member.publication
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    format!(
                        "merge|owner={}|name={}|symbol={}|disposition={:?}|fragments={fragments}|members={members}",
                        context.declaration_owner(attachment.owner),
                        attachment.name,
                        context.symbol(attachment.symbol),
                        attachment.disposition
                    )
                })
        })
        .collect::<Vec<_>>();
    attachments.extend(
        binder
            .all_standalone_namespace_value_attachments()
            .into_iter()
            .map(|attachment| {
                let fragments = attachment
                    .fragments
                    .iter()
                    .map(|fragment| {
                        format!(
                            "{}:{}:{}",
                            context.namespace(fragment.namespace),
                            context.source(fragment.source),
                            fragment.source_start
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let members = attachment
                    .members
                    .iter()
                    .map(|member| {
                        format!(
                            "{}:{}:{:?}:decl={}:value={}:alias={}:symbol={}:child={}:spaces={:?}",
                            context.source(member.source),
                            member.declaration_span.start,
                            member.name,
                            member
                                .declaration
                                .map(|declaration| context.declaration(declaration))
                                .unwrap_or_else(|| "none".to_owned()),
                            member
                                .value_storage
                                .map(|storage| context.value_storage(storage))
                                .unwrap_or_else(|| "none".to_owned()),
                            member
                                .alias_target_storage
                                .map(|storage| context.value_storage(storage))
                                .unwrap_or_else(|| "none".to_owned()),
                            member
                                .symbol
                                .map(|symbol| context.symbol(symbol))
                                .unwrap_or_else(|| "none".to_owned()),
                            member
                                .child_namespace
                                .map(|namespace| context.namespace(namespace))
                                .unwrap_or_else(|| "none".to_owned()),
                            member.spaces
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    "standalone|namespace={}|storage={}|fragments={fragments}|members={members}",
                    context.namespace(attachment.namespace),
                    context.value_storage(attachment.storage)
                )
            }),
    );
    attachments.sort();

    let mut augmentations = binder
        .namespaces
        .globals()
        .map(|global| {
            let members = global
                .members
                .iter()
                .filter_map(|member| binder.namespaces.member(*member))
                .map(|member| {
                    format!(
                        "{}:{}:{:?}:{:?}:{:?}:target={}:decl={}:symbol={}",
                        context.source(member.source),
                        member.binding_span.start,
                        member.name,
                        member.spaces,
                        member.kind,
                        context.declaration_owner(member.target),
                        member
                            .declaration
                            .map(|id| context.declaration(id))
                            .unwrap_or_else(|| "none".to_owned()),
                        member
                            .symbol
                            .map(|id| context.symbol(id))
                            .unwrap_or_else(|| "none".to_owned())
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{}|{}:{}|{:?}|declared={}|target={}|overlay={}|members={members}",
                context.source(global.source),
                global.body_span.start,
                global.body_span.end,
                global.placement,
                global.declared,
                context.scope(global.target_scope),
                context.scope(global.overlay_scope)
            )
        })
        .collect::<Vec<_>>();
    augmentations.sort();

    let mut global_contributors = binder
        .namespaces
        .merges()
        .filter(|merge| merge.owner == DeclarationOwner::CompilationGlobal)
        .filter(|merge| merge.classification.disposition == MergeDisposition::Admitted)
        .filter_map(|merge| {
            let ordinary = merge.declarations.iter().any(|declaration| {
                declaration.kind == MergeDeclarationKind::Function
                    || matches!(
                        declaration.syntax,
                        DeclarationSyntaxFacts::Variable(VariableKind::Var)
                    )
            });
            let namespace = merge
                .declarations
                .iter()
                .find_map(|declaration| declaration.namespace_fragment)
                .and_then(|fragment| binder.namespaces.fragment(fragment))
                .is_some_and(|fragment| {
                    binder
                        .namespaces
                        .aggregate_instance_state(fragment.namespace)
                        == Some(NamespaceInstanceState::Instantiated)
                });
            (ordinary || namespace).then(|| {
                let mut declarations = merge
                    .declarations
                    .iter()
                    .map(|declaration| {
                        format!(
                            "{}:{}:{:?}:{:?}",
                            context.source(declaration.source),
                            declaration.binding_span.start,
                            declaration.spaces,
                            declaration.kind
                        )
                    })
                    .collect::<Vec<_>>();
                declarations.sort();
                format!("{}|{}", merge.name, declarations.join(","))
            })
        })
        .collect::<Vec<_>>();
    global_contributors.sort();

    let mut modules = bound
        .project_sources
        .iter()
        .map(|row| {
            format!(
                "{}|kind={:?}|external={}",
                row.normalized_path, row.source_file_kind, row.external_module
            )
        })
        .collect::<Vec<_>>();
    modules.extend(
        bound
            .normalized
            .normalized_import_export_shape
            .iter()
            .cloned(),
    );
    modules.sort();

    DeclarationProjection {
        scopes,
        declarations,
        groups,
        namespaces,
        attachments,
        augmentations,
        global_contributors,
        modules,
    }
}

struct ProjectionContext<'a> {
    binder: &'a crate::binder::bind::Binder,
    source_names: BTreeMap<SourceUnitKey, String>,
    module_names: BTreeMap<u32, String>,
}

impl ProjectionContext<'_> {
    fn source(&self, source: SourceUnitKey) -> String {
        self.source_names
            .get(&source)
            .cloned()
            .unwrap_or_else(|| "prelude".to_owned())
    }

    fn scope(&self, scope: ScopeId) -> String {
        if scope == self.binder.prelude_module {
            return "scope:prelude".to_owned();
        }
        if scope == self.binder.compilation_global {
            return "scope:compilation-global".to_owned();
        }
        if scope == self.binder.script_namespace_root {
            return "scope:script-namespace-root".to_owned();
        }
        if let Some(module) = self.module_names.get(&scope.0) {
            return format!("scope:module:{module}");
        }
        if let Some(namespace) = self
            .binder
            .namespaces
            .namespaces()
            .find(|namespace| namespace.public_scope == scope)
        {
            return format!("scope:namespace-public:{}", self.namespace(namespace.id));
        }
        if let Some(fragment) = self
            .binder
            .namespaces
            .fragments()
            .find(|fragment| fragment.private_scope == scope)
        {
            return format!(
                "scope:namespace-private:{}:{}:{}",
                self.namespace(fragment.namespace),
                self.source(fragment.source),
                fragment.source_start
            );
        }
        if let Some(global) = self
            .binder
            .namespaces
            .globals()
            .find(|global| global.overlay_scope == scope)
        {
            return format!(
                "scope:global-overlay:{}:{}",
                self.source(global.source),
                global.body_span.start
            );
        }
        if let Some(((module, start), _)) = self
            .binder
            .fn_scopes
            .iter()
            .find(|(_, candidate)| **candidate == scope)
        {
            return format!("scope:function:{}:{start}", self.scope(*module));
        }
        if let Some(((module, start), _)) = self
            .binder
            .block_scopes
            .iter()
            .find(|(_, candidate)| **candidate == scope)
        {
            return format!("scope:block:{}:{start}", self.scope(*module));
        }
        let kind = self
            .binder
            .graph
            .get(scope)
            .map(|row| format!("{:?}", row.kind))
            .unwrap_or_else(|| "missing".to_owned());
        format!("scope:unanchored:{kind}")
    }

    fn namespace(&self, namespace: NamespaceId) -> String {
        self.binder
            .namespaces
            .get(namespace)
            .map(|namespace| {
                format!(
                    "{}::{}",
                    self.namespace_owner(namespace.owner),
                    namespace.name
                )
            })
            .unwrap_or_else(|| "namespace:missing".to_owned())
    }

    fn namespace_owner(&self, owner: NamespaceOwner) -> String {
        match owner {
            NamespaceOwner::Lexical(scope) => self.scope(scope),
            NamespaceOwner::NamespacePublic(namespace) => self.namespace(namespace),
            NamespaceOwner::FragmentPrivate(fragment) => self
                .binder
                .namespaces
                .fragment(fragment)
                .map(|fragment| {
                    format!(
                        "{}:private:{}:{}",
                        self.namespace(fragment.namespace),
                        self.source(fragment.source),
                        fragment.source_start
                    )
                })
                .unwrap_or_else(|| "namespace-fragment:missing".to_owned()),
            NamespaceOwner::CompilationGlobal => "scope:compilation-global".to_owned(),
        }
    }

    fn declaration_owner(&self, owner: DeclarationOwner) -> String {
        match owner {
            DeclarationOwner::Lexical(scope) => self.scope(scope),
            DeclarationOwner::NamespacePublic(namespace) => self.namespace(namespace),
            DeclarationOwner::NamespacePrivate(fragment) => self
                .binder
                .namespaces
                .fragment(fragment)
                .map(|fragment| {
                    format!(
                        "{}:private:{}:{}",
                        self.namespace(fragment.namespace),
                        self.source(fragment.source),
                        fragment.source_start
                    )
                })
                .unwrap_or_else(|| "namespace-fragment:missing".to_owned()),
            DeclarationOwner::CompilationGlobal => "scope:compilation-global".to_owned(),
            DeclarationOwner::DeferredAmbientModule(module) => self
                .binder
                .namespaces
                .deferred_modules()
                .find(|candidate| candidate.id == module)
                .map(|module| {
                    format!(
                        "ambient:{}:{}:{}",
                        self.source(module.source),
                        module.span.start,
                        module.specifier
                    )
                })
                .unwrap_or_else(|| "ambient:missing".to_owned()),
        }
    }

    fn declaration(&self, id: crate::binder::declaration::DeclId) -> String {
        self.binder
            .declarations
            .get(id)
            .map(|declaration| {
                let source = self
                    .binder
                    .source_for_module(declaration.site.module)
                    .map(|source| self.source(source))
                    .unwrap_or_else(|| "prelude".to_owned());
                format!(
                    "{source}:{}:{}:{:?}",
                    declaration.site.binding_span.start,
                    declaration.site.binding_span.end,
                    declaration.kind
                )
            })
            .unwrap_or_else(|| "declaration:missing".to_owned())
    }

    fn value_storage(&self, storage: crate::binder::declaration::ValueStorageId) -> String {
        if let Some(declaration) = self
            .binder
            .declarations
            .iter()
            .find(|declaration| declaration.value_storage == Some(storage))
        {
            return format!("declaration:{}", self.declaration(declaration.id));
        }
        self.binder
            .namespaces
            .namespaces()
            .find(|namespace| {
                self.binder
                    .namespaces
                    .standalone_value_storage(namespace.id)
                    == Some(storage)
            })
            .map(|namespace| format!("namespace:{}", self.namespace(namespace.id)))
            .unwrap_or_else(|| "value-storage:missing".to_owned())
    }

    fn type_group(&self, id: crate::binder::declaration::TypeGroupId) -> String {
        self.binder
            .type_groups
            .get(id)
            .map(|group| {
                let fragments = group
                    .fragments
                    .iter()
                    .map(|fragment| {
                        format!(
                            "{}:{}:{:?}:{}",
                            self.source(fragment.source),
                            fragment.site.binding_span.start,
                            fragment.kind,
                            self.scope(fragment.scope)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                format!("{}[{fragments}]", group.name)
            })
            .unwrap_or_else(|| "type-group:missing".to_owned())
    }

    fn symbol(&self, id: crate::binder::symbol::SymbolId) -> String {
        self.binder
            .symbols
            .get(id)
            .map(|symbol| {
                format!(
                    "{}|value={}|type={}|namespace={}",
                    symbol.name,
                    symbol
                        .value
                        .map(|storage| self.value_storage(storage))
                        .unwrap_or_else(|| "none".to_owned()),
                    symbol
                        .ty
                        .map(|group| self.type_group(group))
                        .unwrap_or_else(|| "none".to_owned()),
                    symbol
                        .ns
                        .map(|namespace| self.namespace(namespace))
                        .unwrap_or_else(|| "none".to_owned())
                )
            })
            .unwrap_or_else(|| "symbol:missing".to_owned())
    }
}

fn project_cases() -> Vec<Vec<UserDeltaProjectInputForTest<'static>>> {
    vec![
        vec![input(
            "/candidate/reflect_packaged.ts",
            include_str!("../../../tests/cases/sr_project_local_library_candidate/reflect_packaged.ts"),
        )],
        vec![input(
            "/candidate/array_library_scc.ts",
            include_str!("../../../tests/cases/sr_project_local_library_candidate/array_library_scc.ts"),
        )],
        vec![
            input(
                "/candidate/isolation_a/00_augment.ts",
                include_str!("../../../tests/cases/sr_project_local_library_candidate/isolation_a/00_augment.ts"),
            ),
            input(
                "/candidate/isolation_a/99_consume.ts",
                include_str!("../../../tests/cases/sr_project_local_library_candidate/isolation_a/99_consume.ts"),
            ),
        ],
        vec![
            input(
                "/candidate/isolation_b/00_augment.ts",
                include_str!("../../../tests/cases/sr_project_local_library_candidate/isolation_b/00_augment.ts"),
            ),
            input(
                "/candidate/isolation_b/99_consume.ts",
                include_str!("../../../tests/cases/sr_project_local_library_candidate/isolation_b/99_consume.ts"),
            ),
        ],
        vec![
            input(
                "/candidate/mixed_group_forward/00_script.ts",
                include_str!("../../../tests/cases/sr_project_local_library_candidate/mixed_group_forward/00_script.ts"),
            ),
            input(
                "/candidate/mixed_group_forward/50_module.ts",
                include_str!("../../../tests/cases/sr_project_local_library_candidate/mixed_group_forward/50_module.ts"),
            ),
            input(
                "/candidate/mixed_group_forward/99_consume.ts",
                include_str!("../../../tests/cases/sr_project_local_library_candidate/mixed_group_forward/99_consume.ts"),
            ),
        ],
        vec![
            input(
                "/candidate/mixed_group_reverse/00_consume.ts",
                include_str!("../../../tests/cases/sr_project_local_library_candidate/mixed_group_reverse/00_consume.ts"),
            ),
            input(
                "/candidate/mixed_group_reverse/50_module.ts",
                include_str!("../../../tests/cases/sr_project_local_library_candidate/mixed_group_reverse/50_module.ts"),
            ),
            input(
                "/candidate/mixed_group_reverse/99_script.ts",
                include_str!("../../../tests/cases/sr_project_local_library_candidate/mixed_group_reverse/99_script.ts"),
            ),
        ],
        vec![
            input(
                "/candidate/source_reexport/00_source.ts",
                "export interface WU3SourceExport { value: number }\n",
            ),
            input(
                "/candidate/source_reexport/50_bridge.ts",
                "export { WU3SourceExport } from \"./00_source.js\";\n",
            ),
            input(
                "/candidate/source_reexport/99_consume.ts",
                "import { WU3SourceExport } from \"./50_bridge.js\";\nconst value: WU3SourceExport = { value: 1 };\n",
            ),
        ],
        vec![input(
            "/candidate/b14/global_this_undefined.ts",
            include_str!("../../../tests/cases/b14_full_lib_loading/global_this_undefined.ts"),
        )],
        vec![input(
            "/candidate/b43/class_interface_namespace_orders.ts",
            include_str!("../../../tests/cases/b43_namespaces_declaration_merging/class_interface_namespace_orders.ts"),
        )],
        vec![
            input(
                "/candidate/b103/global_this_contributor/00_global.ts",
                include_str!("../../../tests/cases/b103_library_merge_correctness_project/global_this_contributor/00_global.ts"),
            ),
            input(
                "/candidate/b103/global_this_contributor/99_consume.ts",
                include_str!("../../../tests/cases/b103_library_merge_correctness_project/global_this_contributor/99_consume.ts"),
            ),
        ],
    ]
}

const fn input(path: &'static str, source: &'static str) -> UserDeltaProjectInputForTest<'static> {
    UserDeltaProjectInputForTest { path, source }
}

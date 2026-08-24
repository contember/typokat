//! RED contract for WU3's eager project-local declaration universe.

use super::base::UserDeltaProjectInputForTest;
use super::compiler::LibraryCompilerWorkScopeForTest;
use super::project_binder_projection::{normalize_project_binder, DeclarationProjection};
use super::provider::{
    CompleteSourceDeclarationOracleForTest, InitializationMeasurement, LibraryBaseProvider,
};
use crate::binder::declaration::SourceDeclarationManifestBuildScopeForTest;
use crate::check::checker::events::UserEventReservationScopeForTest;
use crate::check::checker::library_compiler::{
    CanonicalLibraryFrontendWorkScopeForTest, CompleteSourceRouteWorkScopeForTest,
    UserDeltaForkScopeForTest, UserSourceWorkScopeForTest,
};
use crate::check::query::QueryCacheWriteScopeForTest;
use crate::relate::cache::RelationCacheWriteScopeForTest;
use crate::types::layered::LocalRowAllocationScopeForTest;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
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
    let route_work = CompleteSourceRouteWorkScopeForTest::start();
    let inputs = vec![input("/candidate/malformed.ts", "const broken = {")];
    let result = provider.complete_source_declaration_oracle_for_test(&inputs);
    let manifests = manifest_work.finish();
    let work = source_work.finish();
    let route_work = route_work.finish();

    assert!(result.is_err());
    assert_eq!((work.binds, work.checks), (0, 0));
    assert_eq!(manifests, 0);
    assert_eq!(route_work.library_parse_units, 0);
    assert_eq!(route_work.library_bind_units, 0);
    assert_eq!(route_work.user_bind_units, 0);
    assert_eq!(route_work.semantic_publications, 0);
    assert_eq!(route_work.replay_trace_constructions, 0);
    assert_eq!(route_work.replay_plan_constructions, 0);
    assert_eq!(route_work.frozen_base_seals, 0);
    assert_eq!(route_work.library_source_reparses, 0);
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
fn complete_source_oracle_allocates_no_user_event_lifecycle() {
    let events = UserEventReservationScopeForTest::start();
    let receipt = complete_source_oracle(&[input("/candidate/no_event_oracle.ts", "export {};\n")]);

    assert!(!receipt.projection.scopes.is_empty());
    assert_eq!(events.finish(), 0);
}

#[test]
fn complete_source_oracle_calibrates_library_parse_and_bind_work() {
    let route_work = CompleteSourceRouteWorkScopeForTest::start();
    let receipt =
        complete_source_oracle(&[input("/candidate/calibrated_oracle.ts", "export {};\n")]);
    let route_work = route_work.finish();

    assert!(!receipt.projection.scopes.is_empty());
    assert_eq!(route_work.library_parse_units, 82);
    assert_eq!(route_work.library_bind_units, 82);
    assert_eq!(route_work.user_bind_units, 1);
    assert_eq!(route_work.semantic_publications, 0);
    assert_eq!(route_work.library_source_reparses, 0);
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
        assert_eq!(ordinary.metrics.product_consumptions, 1);
        assert_eq!(perturbed.metrics.product_consumptions, 1);
        assert_eq!(
            ordinary.metrics.bound_user_units,
            u64::try_from(inputs.len()).expect("project input count fits u64")
        );
        assert_eq!(
            perturbed.metrics.bound_user_units,
            u64::try_from(inputs.len()).expect("project input count fits u64")
        );
        let _authenticated_binding_wall_us = ordinary.metrics.binding_wall_us;
        assert_ne!(ordinary_rows, 0);
        assert_eq!(
            perturbed_rows,
            ordinary_rows
                .checked_add(PERTURB_LOCAL_ROW_DELTA)
                .expect("fresh-binder row receipt fits u64")
        );
        assert_projection_eq(&normalize_candidate(&ordinary), &oracle);
        assert_projection_eq(&normalize_candidate(&perturbed), &oracle);
        assert_ne!(
            ordinary.bound.binder.checkpoint_ends(),
            perturbed.bound.binder.checkpoint_ends()
        );
        if let Some((reversed, reversed_rows)) = &reversed {
            assert!(Arc::ptr_eq(&product, &reversed.product));
            assert_eq!(*reversed_rows, ordinary_rows);
            assert_projection_eq(&normalize_candidate(reversed), &oracle);
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
fn packaged_product_map_tracks_a_live_array_reopen_after_legacy_id_perturbation() {
    let provider = LibraryBaseProvider::new();
    let candidate = build_candidate_for_test(
        &provider,
        &[input(
            "/candidate/live_array_reopen.ts",
            "interface Array<T> { wu4PackagedValue?: T }\n",
        )],
        true,
    )
    .expect("packaged product survives a live Array reopen");
    let map = candidate
        .bound
        .library_product_map_for_test()
        .expect("product map");
    let (group_index, recipe) = candidate
        .product
        .groups()
        .iter()
        .enumerate()
        .find(|(_, group)| {
            candidate.product.strings()[group.name.index()] == "Array"
                && candidate.product.containers()[group.container.index()].kind
                    == typokat_library_product::ContainerKind::CompilationGlobal
        })
        .expect("global Array product group");
    let key = candidate
        .product
        .group_key_at(group_index)
        .expect("Array product key");
    let binding = map.group(key).expect("mapped Array group");
    let symbol = candidate
        .bound
        .binder
        .symbols
        .get(binding.symbol())
        .expect("final Array symbol");
    assert_eq!(symbol.ty, binding.ty());
    assert_eq!(symbol.value, binding.value());
    assert_eq!(symbol.ns, binding.namespace());
    let ty = binding.ty().expect("Array type group");
    assert!(
        candidate
            .bound
            .binder
            .type_groups
            .get(ty)
            .expect("final Array type group")
            .fragments
            .len()
            > recipe.fragments.len(),
        "final mapped Array group includes the live user fragment"
    );
}

#[test]
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
    metrics: super::provider::ProjectDeclarationUniverseMetricsForTest,
}

fn build_candidate_for_test(
    provider: &LibraryBaseProvider,
    inputs: &[UserDeltaProjectInputForTest<'_>],
    perturb_local_ids: bool,
) -> Result<CandidateReceipt, String> {
    let prepared =
        provider.prepare_project_declaration_universe_for_test(inputs, perturb_local_ids)?;
    Ok(CandidateReceipt {
        product: prepared.product,
        library_units: prepared.library_units,
        bound: prepared.bound,
        metrics: prepared.metrics,
    })
}

fn normalize_candidate(candidate: &CandidateReceipt) -> DeclarationProjection {
    normalize_project_binder(&candidate.library_units, &candidate.bound)
}

fn assert_projection_eq(actual: &DeclarationProjection, expected: &DeclarationProjection) {
    if actual == expected {
        return;
    }
    for (kind, actual_rows, expected_rows) in [
        ("scope", &actual.scopes, &expected.scopes),
        ("declaration", &actual.declarations, &expected.declarations),
        ("group", &actual.groups, &expected.groups),
        ("namespace", &actual.namespaces, &expected.namespaces),
        ("attachment", &actual.attachments, &expected.attachments),
        (
            "augmentation",
            &actual.augmentations,
            &expected.augmentations,
        ),
        (
            "global_contributor",
            &actual.global_contributors,
            &expected.global_contributors,
        ),
        ("module", &actual.modules, &expected.modules),
    ] {
        if actual_rows == expected_rows {
            continue;
        }
        let first = actual_rows
            .iter()
            .zip(expected_rows)
            .position(|(actual, expected)| actual != expected)
            .unwrap_or_else(|| actual_rows.len().min(expected_rows.len()));
        panic!(
            "{kind} projection mismatch: actual_count={}, expected_count={}, actual_digest={}, expected_digest={}, first_index={first}, actual={:?}, expected={:?}",
            actual_rows.len(),
            expected_rows.len(),
            actual.digest(),
            expected.digest(),
            actual_rows.get(first),
            expected_rows.get(first),
        );
    }
    unreachable!("projection inequality has a differing row class");
}

fn assert_candidate_implementation_boundary() {
    let source = include_str!("provider.rs");
    let start = source
        .find("pub(super) fn prepare_project_declaration_universe_for_test")
        .expect("candidate provider entry exists");
    let body = rust_function_body(source, start);
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
    normalize_project_binder(&oracle.library_units, &oracle.bound)
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

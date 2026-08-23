use super::declaration_product::{
    build_packaged_declaration_product, build_packaged_declaration_product_with_order_for_test,
};
use super::profile::ExactLibraryProfile;
use crate::binder::declaration::SourceDeclarationManifestBuildScopeForTest;
use crate::check::checker::library_compiler::{
    compile_library_binder_checkpoint, InjectedLibrarySource,
};
use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_ast_visit::Visit;
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType};
use std::collections::BTreeMap;
use std::sync::Arc;
use typokat_library_product::{
    ContainerKind, DependencyKind, ProductFieldClass, ProductFieldVisitor, TypeRecipeNode,
    TypeReferenceResolution,
};

fn assert_owned_product<T: Send + Sync + 'static>(_: &T) {}

#[derive(Default)]
struct ProductFieldCensus {
    owned_text: usize,
    product_key: usize,
    product_key_list: usize,
    scalar: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct StableSyntaxDependencyRow {
    source_ordinal: usize,
    start: u32,
    end: u32,
    kind: &'static str,
}

struct IndependentDependencyVisitor {
    source_ordinal: usize,
    rows: Vec<StableSyntaxDependencyRow>,
}

impl IndependentDependencyVisitor {
    fn push(&mut self, span: oxc_span::Span, kind: &'static str) {
        self.rows.push(StableSyntaxDependencyRow {
            source_ordinal: self.source_ordinal,
            start: span.start,
            end: span.end,
            kind,
        });
    }
}

impl<'ast> Visit<'ast> for IndependentDependencyVisitor {
    fn enter_node(&mut self, kind: AstKind<'ast>) {
        match kind {
            AstKind::TSTypeReference(value) => self.push(value.span, "reference"),
            AstKind::TSInterfaceHeritage(value) => self.push(value.span, "heritage"),
            AstKind::TSTypeQuery(value) => self.push(value.span, "query"),
            AstKind::TSMethodSignature(value) if value.computed => {
                self.push(value.key.span(), "query")
            }
            AstKind::TSPropertySignature(value) if value.computed => {
                self.push(value.key.span(), "query")
            }
            AstKind::MethodDefinition(value) if value.computed => {
                self.push(value.key.span(), "query")
            }
            AstKind::PropertyDefinition(value) if value.computed => {
                self.push(value.key.span(), "query")
            }
            _ => {}
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct StableAttachmentRow {
    source_ordinal: usize,
    declaration_start: u32,
    binding_start: u32,
    slot: &'static str,
}

fn packaged_sources(profile: &ExactLibraryProfile) -> Vec<InjectedLibrarySource<'_>> {
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

fn rows_digest<T: std::fmt::Debug>(rows: &[T]) -> String {
    let rendered = rows
        .iter()
        .enumerate()
        .map(|(row, value)| format!("{row}|{value:?}\n"))
        .collect::<String>();
    typokat_library_product::sha256_hex(rendered.as_bytes())
}

impl ProductFieldVisitor for ProductFieldCensus {
    fn visit_field(
        &mut self,
        class: ProductFieldClass,
        _table: &'static str,
        _row: usize,
        _field: &'static str,
    ) {
        match class {
            ProductFieldClass::OwnedText => self.owned_text += 1,
            ProductFieldClass::ProductKey => self.product_key += 1,
            ProductFieldClass::ProductKeyList => self.product_key_list += 1,
            ProductFieldClass::Scalar => self.scalar += 1,
        }
    }
}

#[test]
fn product_receipt_matches_authenticated_edge_and_attachment_oracles() {
    let product = build_packaged_declaration_product().expect("packaged declaration product");
    product.validate().expect("closed product validates");
    let receipt = product.audit_receipt();
    let mut walked = ProductFieldCensus::default();
    product.walk_fields(&mut walked);
    assert!(walked.owned_text > 0);
    assert!(walked.product_key > 0);
    assert!(walked.product_key_list > 0);
    assert!(walked.scalar > 0);
    eprintln!("{receipt:#?}");
    assert_eq!(receipt.source_count, 82);
    assert_eq!(receipt.container_count, 6);
    assert_eq!(receipt.declaration_count, 13_958);
    assert_eq!(receipt.group_count, 3_406);
    assert_eq!(receipt.region_count, 97);
    assert_eq!(receipt.callable_scope_count, 153);
    assert_eq!(receipt.type_parameter_count, 1_044);
    assert_eq!(receipt.type_recipe_count, 41_853);
    assert_eq!(receipt.signature_count, 6_683);
    assert_eq!(receipt.member_count, 14_234);
    assert_eq!(receipt.lexical_step_count, 26_364);
    assert_eq!(receipt.dependency_edge_count, 16_451);
    assert_eq!(receipt.namespace_attachment_count, 14);
    assert_eq!(receipt.value_attachment_count, 1_318);
    assert_eq!(receipt.index_signature_parameter_count, 72);
    assert_eq!(receipt.computed_property_name_count, 141);
    assert_eq!(receipt.constructor_count, 2);
    assert_eq!(receipt.private_constructor_count, 2);
    assert_eq!(receipt.abstract_class_count, 1);
    assert_eq!(receipt.abstract_member_count, 1);
    assert_eq!(receipt.resolved_group_reference_count, 14_087);
    assert_eq!(receipt.type_parameter_reference_count, 2_207);
    assert_eq!(receipt.unresolved_reference_count, 3);
    assert_eq!(receipt.resolved_group_dependency_count, 14_235);
    assert_eq!(receipt.type_parameter_dependency_count, 2_207);
    assert_eq!(receipt.unresolved_dependency_count, 3);
    assert_eq!(receipt.canonical_bytes, 30_688_072);
    assert_eq!(
        receipt.canonical_digest,
        "a3e543680440ed88755d4d43f5a5a02c071052a061705e378d29d03a313d6c82"
    );
    assert_eq!(
        receipt.dependency_edge_digest,
        "9804f3438bf2293a705a71fc4238f18665b0f2a1cf34589a56e04f238f98598a"
    );
    assert_eq!(
        receipt.dependency_scc_digest,
        "a52538ebfe3231d1345ca23343421fac509afde19df35ce1f9635710c7837856"
    );
    assert_eq!(
        receipt.lexical_order_digest,
        "a622952bc63c20ebf7243543051fb0a899fd83d4875cb922af1f14eecf813917"
    );
    assert_eq!(
        receipt.callable_scope_digest,
        "34820efc0bdc8e3ff7e9a004b8b00c01715ca8331f789da6e3582c42c3b77c6c"
    );
    assert_eq!(
        receipt.namespace_attachment_digest,
        "44af5e5f929565fb59dd7eea773256729227f002bb8b82fd402856cbdf4cb606"
    );
    assert_eq!(
        receipt.value_attachment_digest,
        "0a60c72c1311a124160dace3837d43358ce7e0736c00a7ec8a430626b8caa848"
    );

    assert!(product.groups().iter().any(|group| {
        let mut sources = group
            .fragments
            .iter()
            .map(|fragment| product.declarations()[fragment.index()].site.source.index())
            .collect::<Vec<_>>();
        sources.sort_unstable();
        sources.dedup();
        sources.len() > 1
            && group.fragments.iter().all(|fragment| {
                let declaration = &product.declarations()[fragment.index()];
                product.regions()[declaration.region.index()].container == group.container
            })
    }));
    assert!(product
        .containers()
        .iter()
        .enumerate()
        .filter(|(_, container)| {
            matches!(
                container.kind,
                ContainerKind::Namespace | ContainerKind::AmbientExternalModule
            )
        })
        .any(|(container, _)| {
            let mut sources = product
                .regions()
                .iter()
                .filter(|region| region.container.index() == container)
                .map(|region| region.site.source.index())
                .collect::<Vec<_>>();
            sources.sort_unstable();
            sources.dedup();
            sources.len() > 1
        }));

    let lib_es5 = product
        .sources()
        .iter()
        .position(|source| product.strings()[source.name.index()] == "lib.es5.d.ts")
        .expect("lib.es5 source");
    for helper in [
        "ThisParameterType",
        "OmitThisParameter",
        "Parameters",
        "ConstructorParameters",
    ] {
        let declaration = product
            .declarations()
            .iter()
            .find(|declaration| {
                declaration.site.source.index() == lib_es5
                    && product.strings()[declaration.name.index()] == helper
            })
            .unwrap_or_else(|| panic!("missing {helper}"));
        let mut resolved_parameters = 0usize;
        let mut unresolved_infer_names = 0usize;
        for recipe in product.types().iter().filter(|recipe| {
            recipe.site.source.index() == lib_es5
                && declaration.site.start <= recipe.site.start
                && recipe.site.end <= declaration.site.end
        }) {
            if let TypeRecipeNode::Reference {
                path,
                resolution,
                arguments: _,
                dependency: _,
            } = &recipe.node
            {
                resolved_parameters += usize::from(matches!(
                    resolution,
                    TypeReferenceResolution::TypeParameter(_)
                ));
                unresolved_infer_names += usize::from(
                    matches!(
                        resolution,
                        TypeReferenceResolution::Unresolved { region: _ }
                    ) && path
                        .iter()
                        .any(|name| matches!(product.strings()[name.index()].as_str(), "P" | "R")),
                );
            }
        }
        assert!(
            resolved_parameters > 0,
            "{helper} has no type-parameter refs"
        );
        assert_eq!(unresolved_infer_names, 0, "{helper} leaked infer P/R");
    }
}

#[test]
fn product_rows_match_independent_ast_and_complete_source_oracles() {
    let profile = ExactLibraryProfile::load_packaged().expect("packaged profile");
    let product = build_packaged_declaration_product().expect("packaged declaration product");

    let mut syntax_rows = Vec::new();
    for source in profile.sources().iter() {
        let allocator = Allocator::default();
        let source_text = std::str::from_utf8(source.bytes()).expect("packaged source is UTF-8");
        let parsed = Parser::new(&allocator, source_text, SourceType::d_ts()).parse();
        assert!(!parsed.panicked, "{} parse panicked", source.name());
        assert!(
            parsed.diagnostics.is_empty(),
            "{} parse diagnostics: {:?}",
            source.name(),
            parsed.diagnostics
        );
        let mut visitor = IndependentDependencyVisitor {
            source_ordinal: source.ordinal().index(),
            rows: Vec::new(),
        };
        visitor.visit_program(&parsed.program);
        syntax_rows.extend(visitor.rows);
    }
    syntax_rows.sort();
    let syntax_counts =
        syntax_rows
            .iter()
            .fold(BTreeMap::<&'static str, usize>::new(), |mut counts, row| {
                *counts.entry(row.kind).or_default() += 1;
                counts
            });
    assert_eq!(syntax_counts.get("reference"), Some(&15_429));
    assert_eq!(syntax_counts.get("heritage"), Some(&868));
    assert_eq!(syntax_counts.get("query"), Some(&(13 + 141)));
    assert_eq!(syntax_rows.len(), 15_429 + 868 + 13 + 141);

    let mut product_dependency_rows = product
        .dependency_rows()
        .iter()
        .map(|dependency| StableSyntaxDependencyRow {
            source_ordinal: usize::try_from(
                product.sources()[dependency.site.source.index()].ordinal,
            )
            .expect("source ordinal fits usize"),
            start: dependency.site.start,
            end: dependency.site.end,
            kind: match dependency.kind {
                DependencyKind::Reference => "reference",
                DependencyKind::Heritage => "heritage",
                DependencyKind::Query => "query",
                DependencyKind::Import => "import",
                DependencyKind::IndexedAccess => "indexed-access",
                DependencyKind::Constraint => "constraint",
            },
        })
        .collect::<Vec<_>>();
    product_dependency_rows.sort();
    assert_eq!(product_dependency_rows, syntax_rows);
    assert_eq!(
        rows_digest(&product_dependency_rows),
        rows_digest(&syntax_rows)
    );

    let injected = packaged_sources(&profile);
    let checkpoint =
        compile_library_binder_checkpoint(&injected).expect("complete-source checkpoint");
    let mut binder_attachments = checkpoint
        .declaration_attachment_rows_for_test()
        .into_iter()
        .flat_map(|row| {
            let value = row.has_value_storage.then_some(StableAttachmentRow {
                source_ordinal: row.library_ordinal.index(),
                declaration_start: row.declaration_start,
                binding_start: row.binding_start,
                slot: "value",
            });
            let namespace = row.has_namespace.then_some(StableAttachmentRow {
                source_ordinal: row.library_ordinal.index(),
                declaration_start: row.declaration_start,
                binding_start: row.binding_start,
                slot: "namespace",
            });
            value.into_iter().chain(namespace)
        })
        .collect::<Vec<_>>();
    binder_attachments.sort();

    let mut product_attachments = product
        .value_attachment_rows()
        .iter()
        .map(|attachment| {
            let declaration = &product.declarations()[attachment.declaration.index()];
            StableAttachmentRow {
                source_ordinal: usize::try_from(
                    product.sources()[declaration.site.source.index()].ordinal,
                )
                .expect("source ordinal fits usize"),
                declaration_start: declaration.site.start,
                binding_start: declaration.binding_site.start,
                slot: "value",
            }
        })
        .chain(
            product
                .namespace_attachment_rows()
                .iter()
                .map(|attachment| {
                    let declaration = &product.declarations()[attachment.declaration.index()];
                    StableAttachmentRow {
                        source_ordinal: usize::try_from(
                            product.sources()[declaration.site.source.index()].ordinal,
                        )
                        .expect("source ordinal fits usize"),
                        declaration_start: declaration.site.start,
                        binding_start: declaration.binding_site.start,
                        slot: "namespace",
                    }
                }),
        )
        .collect::<Vec<_>>();
    product_attachments.sort();
    assert_eq!(product_attachments, binder_attachments);
    assert_eq!(
        rows_digest(&product_attachments),
        rows_digest(&binder_attachments)
    );
}

#[test]
fn product_builds_one_manifest_per_source() {
    let scope = SourceDeclarationManifestBuildScopeForTest::start();
    let product = build_packaged_declaration_product().expect("packaged declaration product");
    assert_eq!(scope.finish(), 82);
    assert_eq!(product.source_count(), 82);
}

#[test]
fn product_drops_ast_allocator_and_binder() {
    let provider = super::provider::LibraryBaseProvider::new();
    provider.get().expect("existing base initializes");
    assert!(!provider.declaration_product_initialized_for_test());
    let lazy_product = provider
        .declaration_product()
        .expect("declaration product initializes independently");
    assert!(provider.declaration_product_initialized_for_test());
    assert_eq!(lazy_product.source_count(), 82);
    let product = build_packaged_declaration_product().expect("packaged declaration product");
    assert_owned_product(&product);
    let second_owner = Arc::clone(&product);
    drop(product);
    assert_eq!(second_owner.source_count(), 82);
}

#[test]
fn product_is_repeat_reverse_shuffle_and_process_deterministic() {
    if std::env::var_os("TYPOKAT_PRODUCT_CHILD").is_some() {
        let product = build_packaged_declaration_product().expect("child product");
        println!(
            "child-product-digest={}",
            product.audit_receipt().canonical_digest
        );
        return;
    }
    let first = build_packaged_declaration_product().expect("first product");
    let second = build_packaged_declaration_product().expect("second product");
    let reverse = build_packaged_declaration_product_with_order_for_test(true, false, false)
        .expect("reverse product");
    let shuffle = build_packaged_declaration_product_with_order_for_test(false, true, false)
        .expect("shuffle product");
    let perturbation = (0..10_000)
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    let perturbed = build_packaged_declaration_product_with_order_for_test(true, true, true)
        .expect("perturbed product");
    drop(perturbation);
    assert_eq!(first.audit_receipt(), second.audit_receipt());
    assert_eq!(first.audit_receipt(), reverse.audit_receipt());
    assert_eq!(first.audit_receipt(), shuffle.audit_receipt());
    assert_eq!(first.audit_receipt(), perturbed.audit_receipt());
    let child = std::process::Command::new(std::env::current_exe().expect("test executable"))
        .args(["library_declaration_product_integration_spec::product_is_repeat_reverse_shuffle_and_process_deterministic", "--exact", "--nocapture"])
        .env("TYPOKAT_PRODUCT_CHILD", "1")
        .output()
        .expect("fresh product process");
    assert!(
        child.status.success(),
        "fresh process failed: {}",
        String::from_utf8_lossy(&child.stderr)
    );
    assert!(
        String::from_utf8_lossy(&child.stdout).contains(&first.audit_receipt().canonical_digest)
    );
}

#[test]
fn product_rejects_forbidden_schema_controls() {
    let product = build_packaged_declaration_product().expect("packaged declaration product");
    product.validate().expect("packaged product validates");
    let mut broken = typokat_library_product::ProductAssembly::new();
    let root = broken.next_region_key().expect("reserved root");
    let name = broken.intern("broken.d.ts").expect("name");
    let digest = broken.intern("00").expect("digest");
    broken
        .add_source(typokat_library_product::SourceRecipe {
            ordinal: 0,
            name,
            sha256: digest,
            byte_len: 0,
            classification: typokat_library_product::SourceClassification::Script,
            root_region: root,
        })
        .expect("source");
    assert!(broken.finish("broken").is_err());
}

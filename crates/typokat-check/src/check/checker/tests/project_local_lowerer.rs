use crate::binder::bind::ProjectBinderBuilder;
use crate::binder::declaration::SourceDeclarationManifest;
use crate::binder::namespace::{
    source_file_kind, CompilationUnit, ModuleBindingContext, SourceUnitKey,
};
use crate::binder::product::{project_library_declaration_product, LibraryProductSource};
use crate::check::checker::project_local::{
    accumulate_product_library_records_for_test, lower_product_groups_for_test,
};
use crate::check::query::{PublishedClassLookup, SemanticQueryCoordinator, SemanticQueryState};
use crate::class_semantics::{DemandOutcome, PublishedClassSurface, PublishedClasses};
use crate::relate::relation::RelationOutcome;
use crate::source::{CompilationOrigin, LibraryFileOrdinal};
use crate::types::repr::{ClassId, ModifierOp, ObjectType, PropertyType, TypeTag};
use crate::types::{Interner, PropertyKey};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use typokat_library_product::{
    ContainerKind, ContainerRecipe, DeclarationFlags, DeclarationGroupRecipe, DeclarationKey,
    DeclarationLexicalScope, DeclarationPayload, DeclarationRecipe, DeclarationSlots,
    DependencyKind, DependencyLinkRecipe, DependencyRecipe, DependencyTarget, GroupKey,
    IntrinsicMarker, IntrinsicMarkerRecipe, KeywordKind, LexicalStepKind, LexicalStepRecipe,
    LibraryDeclarationProduct, MappedModifier, MemberKey, MemberKind, MemberRecipe,
    ParameterRecipe, ProductAssembly, PropertyNameRecipe, RegionKey, RegionKind, RegionRecipe,
    SignatureRecipe, SourceClassification, SourceKey, SourceRecipe, SourceSite,
    TypeParameterRecipe, TypeRecipe, TypeRecipeKey, TypeRecipeNode, TypeReferenceResolution,
    ValueAttachmentRecipe, ValueReferenceResolution, Variance,
};
use typokat_types::alias_semantics::{
    PublishedAliasTerminal, PublishedAliasUnavailable, PublishedAliases,
};

struct AliasPublication {
    classes: PublishedClasses,
    aliases: PublishedAliases,
}

impl PublishedClassLookup for AliasPublication {
    fn published_class(&self, class: ClassId) -> DemandOutcome<&PublishedClassSurface> {
        self.classes.published_class(class)
    }

    fn require_class(&self, class: ClassId) -> DemandOutcome<()> {
        self.classes.require(class)
    }

    fn publication_identity(&self) -> &Arc<()> {
        self.aliases.identity()
    }

    fn published_alias(
        &self,
        alias: crate::types::repr::AliasId,
    ) -> Option<&PublishedAliasTerminal> {
        self.aliases.terminal(alias)
    }
}

struct Fixture {
    assembly: ProductAssembly,
    source: SourceKey,
    root: RegionKey,
    container: typokat_library_product::ContainerKey,
}

impl Fixture {
    fn new() -> Self {
        let mut assembly = ProductAssembly::new();
        let source = assembly.next_source_key().expect("source key");
        let root = assembly.next_region_key().expect("root region");
        let container = assembly
            .add_container(ContainerRecipe {
                parent: None,
                kind: ContainerKind::CompilationGlobal,
                name: None,
                anchor: None,
            })
            .expect("global container");
        let source_name = assembly
            .intern("deferred-member.d.ts")
            .expect("source name");
        let digest = assembly.intern("00").expect("digest");
        assembly
            .add_source(SourceRecipe {
                ordinal: 0,
                name: source_name,
                sha256: digest,
                byte_len: 512,
                classification: SourceClassification::Script,
                root_region: root,
            })
            .expect("source");
        assembly
            .add_region(RegionRecipe {
                parent: None,
                container,
                kind: RegionKind::Source,
                site: SourceSite {
                    source,
                    start: 0,
                    end: 512,
                },
            })
            .expect("root region");
        Self {
            assembly,
            source,
            root,
            container,
        }
    }

    const fn site(&self, start: u32, end: u32) -> SourceSite {
        SourceSite {
            source: self.source,
            start,
            end,
        }
    }

    fn keyword(&mut self, keyword: KeywordKind, start: u32) -> TypeRecipeKey {
        self.assembly
            .add_type_recipe(TypeRecipe {
                site: self.site(start, start + 4),
                node: TypeRecipeNode::Keyword(keyword),
            })
            .expect("keyword recipe")
    }

    fn property(&mut self, name: &str, value: TypeRecipeKey, start: u32) -> MemberKey {
        let name = self.assembly.intern(name).expect("property name");
        self.assembly
            .add_member(MemberRecipe {
                site: self.site(start, start + 8),
                kind: MemberKind::Property,
                name: Some(PropertyNameRecipe::Identifier(name)),
                optional: false,
                readonly: false,
                static_member: false,
                accessibility: None,
                r#abstract: false,
                value: Some(value),
                signature: None,
            })
            .expect("property member")
    }

    fn method(&mut self, name: &str, result: TypeRecipeKey, start: u32) -> MemberKey {
        let name = self.assembly.intern(name).expect("method name");
        let signature = self
            .assembly
            .add_signature(SignatureRecipe {
                type_parameters: Box::new([]),
                parameters: Box::new([]),
                result: Some(result),
            })
            .expect("method signature");
        self.assembly
            .add_member(MemberRecipe {
                site: self.site(start, start + 8),
                kind: MemberKind::Method,
                name: Some(PropertyNameRecipe::Identifier(name)),
                optional: false,
                readonly: false,
                static_member: false,
                accessibility: None,
                r#abstract: false,
                value: None,
                signature: Some(signature),
            })
            .expect("method member")
    }

    fn computed_property(
        &mut self,
        owner: GroupKey,
        name: &str,
        value: TypeRecipeKey,
        start: u32,
    ) -> MemberKey {
        let name = self.assembly.intern(name).expect("computed name");
        let site = self.site(start + 1, start + 5);
        self.assembly.add_dependency(DependencyRecipe {
            owner,
            target: DependencyTarget::IntrinsicGlobalValue,
            kind: DependencyKind::Query,
            site,
        });
        self.assembly
            .add_member(MemberRecipe {
                site: self.site(start, start + 8),
                kind: MemberKind::Property,
                name: Some(PropertyNameRecipe::Computed {
                    site,
                    value: typokat_library_product::SymbolicValueNameRecipe::Path {
                        path: Box::new([name]),
                        resolution: ValueReferenceResolution::IntrinsicGlobalValue,
                        dependency: DependencyLinkRecipe {
                            owner,
                            kind: DependencyKind::Query,
                        },
                    },
                }),
                optional: false,
                readonly: false,
                static_member: false,
                accessibility: None,
                r#abstract: false,
                value: Some(value),
                signature: None,
            })
            .expect("computed property")
    }

    fn structural_declaration(
        &mut self,
        name: &str,
        payload: DeclarationPayload,
        spaces: DeclarationSlots,
        start: u32,
        member_sites: &[SourceSite],
    ) -> DeclarationKey {
        let name = self.assembly.intern(name).expect("declaration name");
        let declaration = self
            .assembly
            .add_declaration(DeclarationRecipe {
                site: self.site(start, start + 180),
                binding_site: self.site(start + 1, start + 2),
                region: self.root,
                lexical_scope: DeclarationLexicalScope::Region(self.root),
                name,
                spaces,
                flags: DeclarationFlags {
                    ambient: true,
                    ..DeclarationFlags::default()
                },
                payload,
            })
            .expect("structural declaration");
        self.assembly.add_lexical_step(LexicalStepRecipe {
            owner: declaration,
            kind: LexicalStepKind::Declaration,
            site: self.site(start, start + 180),
        });
        for site in member_sites {
            self.assembly.add_lexical_step(LexicalStepRecipe {
                owner: declaration,
                kind: LexicalStepKind::Member,
                site: *site,
            });
        }
        declaration
    }

    fn group(
        &mut self,
        name: &str,
        slots: DeclarationSlots,
        fragments: Vec<DeclarationKey>,
    ) -> GroupKey {
        let name = self.assembly.intern(name).expect("group name");
        self.assembly
            .add_group(DeclarationGroupRecipe {
                container: self.container,
                name,
                slots,
                fragments: fragments.into_boxed_slice(),
            })
            .expect("group")
    }

    fn finish(self, profile: &str) -> LibraryDeclarationProduct {
        self.assembly.finish(profile).expect("valid test product")
    }
}

const TYPE_ONLY: DeclarationSlots = DeclarationSlots {
    value: false,
    r#type: true,
    namespace: false,
};

fn extract_library_product(source: &str, profile: &str) -> LibraryDeclarationProduct {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::d_ts()).parse();
    assert!(
        parsed.diagnostics.is_empty(),
        "source fixture parses cleanly"
    );
    let manifest = SourceDeclarationManifest::build(&parsed.program);
    let prelude_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
    let mut builder = ProjectBinderBuilder::new_empty_library_prelude(&prelude.program);
    let unit = CompilationUnit {
        source: SourceUnitKey(1),
        origin: CompilationOrigin::Library(LibraryFileOrdinal::new(0)),
        binding: ModuleBindingContext::for_program(
            &parsed.program,
            source_file_kind("project-local-lowerer.d.ts"),
        ),
    };
    let modules = builder
        .try_add_library_modules_with_declaration_manifests(&[(&parsed.program, &manifest, unit)])
        .expect("bind source-extracted product");
    let binder = builder.finish(modules[0]);
    project_library_declaration_product(
        profile,
        &binder,
        &[LibraryProductSource {
            ordinal: 0,
            name: "project-local-lowerer.d.ts",
            source_sha256: "00",
            byte_len: u32::try_from(source.len()).expect("source length"),
            external_module: false,
            module: modules[0],
            manifest: &manifest,
            source_text: source,
        }],
        &[],
    )
    .expect("extract source product")
}

fn product_group(product: &LibraryDeclarationProduct, name: &str) -> usize {
    product
        .groups()
        .iter()
        .position(|group| product.strings()[group.name.index()] == name)
        .unwrap_or_else(|| panic!("missing source-extracted group {name}"))
}

#[test]
fn source_mapped_alias_preserves_active_key_and_modifiers() {
    let product = extract_library_product(
        "type R<T> = { readonly [P in keyof T]: T[P] };",
        "mapped-alias-contract",
    );
    let group = product_group(&product, "R");
    let declaration = product.groups()[group].fragments[0];
    let DeclarationPayload::TypeAlias { value, .. } =
        &product.declarations()[declaration.index()].payload
    else {
        panic!("R is a type alias")
    };
    let TypeRecipeNode::Mapped {
        optional, readonly, ..
    } = &product.types()[value.index()].node
    else {
        panic!("R has a mapped body")
    };
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[group],
        &BTreeMap::new(),
        [None, None],
        None,
    );
    let template = receipt.group_types[group].expect("ready alias template");
    let alias = interner
        .store()
        .alias_template_id(template)
        .expect("published alias identity");
    let PublishedAliasTerminal::Ready(surface) = receipt
        .aliases
        .terminal(alias)
        .expect("published alias terminal")
    else {
        panic!("mapped alias is ready")
    };
    let mapped = interner
        .store()
        .mapped_type(surface.body())
        .copied()
        .expect("published alias body is mapped");

    assert!(mapped.homomorphic);
    assert_eq!(interner.store().tag(mapped.key_source), TypeTag::TypeParam);
    assert_eq!(
        interner.store().tag(mapped.value_template),
        TypeTag::MappedValue,
        "T[P] on the active mapped key becomes the evaluator placeholder"
    );
    assert_eq!(
        (*optional, *readonly),
        (MappedModifier::Preserve, MappedModifier::Add)
    );
    assert_eq!(mapped.optional_modifier, ModifierOp::Keep);
    assert_eq!(mapped.readonly_modifier, ModifierOp::Add);

    let parameter = surface.type_params()[0];
    let number = interner.well_known().number;
    let source = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", number)],
        ..ObjectType::default()
    });
    let mut expected_property = PropertyType::public("value", number);
    expected_property.readonly = true;
    let expected = interner.intern_object(ObjectType {
        properties: vec![expected_property],
        ..ObjectType::default()
    });
    let application = interner.intern_instantiation(template, vec![(parameter, source)]);
    let published = AliasPublication {
        classes: PublishedClasses::empty(),
        aliases: receipt.aliases,
    };
    let mut state = SemanticQueryState::default();
    let mut next_type_parameter = 1;
    assert!(matches!(
        SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_parameter,
        )
        .is_assignable(application, expected),
        RelationOutcome::Yes
    ));
}

#[test]
fn mapped_alias_rejects_applied_parameters_and_visits_their_arguments() {
    let product = extract_library_product(
        concat!(
            "type BadKey<T> = { [P in keyof T]: T[P<intrinsic>] };",
            "type BadSource<T> = { [P in \"x\"]: T<intrinsic>[P] };",
        ),
        "mapped-applied-parameter-contract",
    );
    let bad_key = product_group(&product, "BadKey");
    let bad_source = product_group(&product, "BadSource");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[bad_key, bad_source],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert!(receipt.unavailable.contains(&bad_key));
    assert!(receipt.unavailable.contains(&bad_source));
    let incomplete_ids = receipt
        .library_records
        .iter()
        .filter_map(|record| match &record.record {
            crate::check::checker::project_local::project_local_pass_port::NormalizedCheckerRecordForTest::Incomplete(incomplete) => Some(incomplete.id.as_str()),
            crate::check::checker::project_local::project_local_pass_port::NormalizedCheckerRecordForTest::Diagnostic(_) => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        incomplete_ids
            .iter()
            .filter(|id| **id == "annotation-lower/type-reference/unresolved")
            .count(),
        2,
        "both rejected parameter applications still visit their unresolved argument"
    );
}

#[test]
fn standalone_interface_discards_deferred_member_and_keeps_supported_sibling() {
    let mut fixture = Fixture::new();
    let number = fixture.keyword(KeywordKind::Number, 102);
    let this = fixture.keyword(KeywordKind::This, 122);
    let keep = fixture.property("keep", number, 100);
    let deferred = fixture.method("sort", this, 120);
    let declaration = fixture.structural_declaration(
        "Surface",
        DeclarationPayload::Interface {
            type_parameters: Box::new([]),
            members: Box::new([keep, deferred]),
            heritage: Box::new([]),
        },
        TYPE_ONLY,
        20,
        &[fixture.site(100, 108), fixture.site(120, 128)],
    );
    let group = fixture.group("Surface", TYPE_ONLY, vec![declaration]);
    let product = fixture.finish("deferred-interface");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[group.index()],
        &BTreeMap::new(),
        [None, None],
        None,
    );
    assert!(receipt.ready.contains(&group.index()));
    let ty = receipt.group_types[group.index()].expect("ready interface type");
    let object = interner.store().object_type(ty).expect("interface object");
    assert_eq!(
        object
            .properties
            .iter()
            .map(|property| property.key.clone())
            .collect::<Vec<_>>(),
        vec![PropertyKey::String("keep".to_owned())]
    );
}

#[test]
fn supported_class_default_stages_no_incomplete() {
    let mut fixture = Fixture::new();
    let number = fixture.keyword(KeywordKind::Number, 42);
    let parameter_name = fixture.assembly.intern("T").expect("parameter name");
    let parameter = fixture
        .assembly
        .add_type_parameter(TypeParameterRecipe {
            site: fixture.site(30, 31),
            owner: fixture.site(20, 180),
            visibility: Box::new([fixture.site(20, 180)]),
            name: parameter_name,
            variance: Variance::Invariant,
            constant: false,
            constraint: None,
            default: Some(number),
        })
        .expect("defaulted parameter");
    let member = fixture.property("keep", number, 100);
    let slots = DeclarationSlots {
        value: true,
        ..TYPE_ONLY
    };
    let class = fixture.structural_declaration(
        "DefaultedClass",
        DeclarationPayload::Class {
            type_parameters: Box::new([parameter]),
            members: Box::new([member]),
            heritage: Box::new([]),
        },
        slots,
        20,
        &[fixture.site(100, 108)],
    );
    let group = fixture.group("DefaultedClass", slots, vec![class]);
    fixture
        .assembly
        .add_value(ValueAttachmentRecipe {
            group,
            declaration: class,
        })
        .expect("class value attachment");
    let product = fixture.finish("supported-class-default");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[group.index()],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert!(receipt.ready.contains(&group.index()));
    assert!(receipt.library_records.is_empty());
}

#[test]
fn unsupported_class_default_stages_once_and_keeps_the_class_atomic() {
    let mut fixture = Fixture::new();
    let intrinsic = fixture.keyword(KeywordKind::Intrinsic, 42);
    let number = fixture.keyword(KeywordKind::Number, 102);
    let parameter_name = fixture.assembly.intern("T").expect("parameter name");
    let parameter = fixture
        .assembly
        .add_type_parameter(TypeParameterRecipe {
            site: fixture.site(30, 31),
            owner: fixture.site(20, 180),
            visibility: Box::new([fixture.site(20, 180)]),
            name: parameter_name,
            variance: Variance::Invariant,
            constant: false,
            constraint: None,
            default: Some(intrinsic),
        })
        .expect("defaulted parameter");
    let member = fixture.property("keep", number, 100);
    let slots = DeclarationSlots {
        value: true,
        ..TYPE_ONLY
    };
    let class = fixture.structural_declaration(
        "UnsupportedDefaultClass",
        DeclarationPayload::Class {
            type_parameters: Box::new([parameter]),
            members: Box::new([member]),
            heritage: Box::new([]),
        },
        slots,
        20,
        &[fixture.site(100, 108)],
    );
    let owner_start = fixture.site(20, 180).start + 1;
    let group = fixture.group("UnsupportedDefaultClass", slots, vec![class]);
    fixture
        .assembly
        .add_value(ValueAttachmentRecipe {
            group,
            declaration: class,
        })
        .expect("class value attachment");
    let product = fixture.finish("unsupported-class-default");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[group.index()],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert!(receipt.unavailable.contains(&group.index()));
    assert_eq!(receipt.group_types[group.index()], None);
    assert_eq!(receipt.library_records.len(), 1);
    let record = &receipt.library_records[0];
    assert_eq!(record.source_path, "deferred-member.d.ts");
    assert_eq!(record.owner_start, owner_start);
    let crate::check::checker::project_local::project_local_pass_port::NormalizedCheckerRecordForTest::Incomplete(incomplete) = &record.record else {
        panic!("unsupported default stages an incomplete")
    };
    assert_eq!(incomplete.span.start, 42);
    assert_eq!(incomplete.span.end, 46);
    assert_eq!(incomplete.id, "annotation-lower/intrinsic-keyword/self");
    assert_eq!(incomplete.context, "intrinsic keyword type not modeled");
}

#[test]
fn unsupported_method_overload_omits_the_whole_property_set() {
    let mut fixture = Fixture::new();
    let number = fixture.keyword(KeywordKind::Number, 102);
    let this = fixture.keyword(KeywordKind::This, 122);
    let supported = fixture.method("overload", number, 100);
    let unsupported = fixture.method("overload", this, 120);
    let sibling = fixture.property("keep", number, 140);
    let declaration = fixture.structural_declaration(
        "Overloads",
        DeclarationPayload::Interface {
            type_parameters: Box::new([]),
            members: Box::new([supported, unsupported, sibling]),
            heritage: Box::new([]),
        },
        TYPE_ONLY,
        20,
        &[
            fixture.site(100, 108),
            fixture.site(120, 128),
            fixture.site(140, 148),
        ],
    );
    let group = fixture.group("Overloads", TYPE_ONLY, vec![declaration]);
    let product = fixture.finish("atomic-overload-set");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[group.index()],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert!(receipt.ready.contains(&group.index()));
    let ty = receipt.group_types[group.index()].expect("ready interface type");
    let object = interner.store().object_type(ty).expect("interface object");
    assert_eq!(
        object
            .properties
            .iter()
            .filter_map(|property| property.key.as_string())
            .collect::<Vec<_>>(),
        vec!["keep"]
    );
}

#[test]
fn unsupported_keyof_in_a_method_never_enters_a_ready_signature() {
    let mut fixture = Fixture::new();
    let number = fixture.keyword(KeywordKind::Number, 124);
    let keyof = fixture
        .assembly
        .add_type_recipe(TypeRecipe {
            site: fixture.site(118, 130),
            node: TypeRecipeNode::Operator {
                operator: typokat_library_product::TypeOperator::Keyof,
                operand: number,
            },
        })
        .expect("keyof recipe");
    let unsupported = fixture.method("keys", keyof, 110);
    let sibling = fixture.property("keep", number, 140);
    let declaration = fixture.structural_declaration(
        "KeyofSurface",
        DeclarationPayload::Interface {
            type_parameters: Box::new([]),
            members: Box::new([unsupported, sibling]),
            heritage: Box::new([]),
        },
        TYPE_ONLY,
        20,
        &[fixture.site(110, 118), fixture.site(140, 148)],
    );
    let group = fixture.group("KeyofSurface", TYPE_ONLY, vec![declaration]);
    let product = fixture.finish("unsupported-keyof");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[group.index()],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert!(receipt.ready.contains(&group.index()));
    let ty = receipt.group_types[group.index()].expect("ready interface type");
    let object = interner.store().object_type(ty).expect("interface object");
    assert_eq!(
        object
            .properties
            .iter()
            .filter_map(|property| property.key.as_string())
            .collect::<Vec<_>>(),
        vec!["keep"]
    );
    assert_eq!(receipt.library_records.len(), 1);
    let crate::check::checker::project_local::project_local_pass_port::NormalizedCheckerRecordForTest::Incomplete(incomplete) = &receipt.library_records[0].record else {
        panic!("unsupported keyof stages an incomplete")
    };
    assert_eq!(
        incomplete.id,
        "annotation-lower/type-operator/keyof-operand"
    );
}

#[test]
fn index_signature_walks_unsupported_key_and_result_before_omission() {
    let mut fixture = Fixture::new();
    let key = fixture.keyword(KeywordKind::This, 122);
    let result = fixture.keyword(KeywordKind::Intrinsic, 142);
    let parameter_name = fixture.assembly.intern("key").expect("parameter name");
    let signature = fixture
        .assembly
        .add_signature(SignatureRecipe {
            type_parameters: Box::new([]),
            parameters: Box::new([ParameterRecipe {
                pattern: typokat_library_product::ParameterPattern::Identifier(parameter_name),
                value: Some(key),
                optional: false,
                rest: false,
                this_parameter: false,
            }]),
            result: Some(result),
        })
        .expect("index signature");
    let index = fixture
        .assembly
        .add_member(MemberRecipe {
            site: fixture.site(110, 150),
            kind: MemberKind::Index,
            name: None,
            optional: false,
            readonly: false,
            static_member: false,
            accessibility: None,
            r#abstract: false,
            value: None,
            signature: Some(signature),
        })
        .expect("index member");
    let number = fixture.keyword(KeywordKind::Number, 162);
    let sibling = fixture.property("keep", number, 160);
    let declaration = fixture.structural_declaration(
        "IndexSurface",
        DeclarationPayload::Interface {
            type_parameters: Box::new([]),
            members: Box::new([index, sibling]),
            heritage: Box::new([]),
        },
        TYPE_ONLY,
        20,
        &[fixture.site(110, 150), fixture.site(160, 168)],
    );
    let group = fixture.group("IndexSurface", TYPE_ONLY, vec![declaration]);
    let product = fixture.finish("index-full-walk");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[group.index()],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert!(receipt.ready.contains(&group.index()));
    assert_eq!(receipt.library_records.len(), 2);
    assert_eq!(
        receipt
            .library_records
            .iter()
            .map(|record| match &record.record {
                crate::check::checker::project_local::project_local_pass_port::NormalizedCheckerRecordForTest::Incomplete(incomplete) => incomplete.id.as_str(),
                crate::check::checker::project_local::project_local_pass_port::NormalizedCheckerRecordForTest::Diagnostic(_) => panic!("unexpected diagnostic"),
            })
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "annotation-lower/intrinsic-keyword/self",
            "annotation-lower/this-type/self",
        ])
    );
    let ty = receipt.group_types[group.index()].expect("ready interface");
    let object = interner.store().object_type(ty).expect("interface object");
    assert!(object.string_index.is_none());
    assert!(object.number_index.is_none());
    assert!(object.property("keep").is_some());
}

#[test]
fn class_index_signature_walks_all_failures_and_keeps_the_group_atomic() {
    let mut fixture = Fixture::new();
    let key = fixture.keyword(KeywordKind::This, 122);
    let result = fixture.keyword(KeywordKind::Intrinsic, 142);
    let parameter_name = fixture.assembly.intern("key").expect("parameter name");
    let signature = fixture
        .assembly
        .add_signature(SignatureRecipe {
            type_parameters: Box::new([]),
            parameters: Box::new([ParameterRecipe {
                pattern: typokat_library_product::ParameterPattern::Identifier(parameter_name),
                value: Some(key),
                optional: false,
                rest: false,
                this_parameter: false,
            }]),
            result: Some(result),
        })
        .expect("index signature");
    let index = fixture
        .assembly
        .add_member(MemberRecipe {
            site: fixture.site(110, 150),
            kind: MemberKind::Index,
            name: None,
            optional: false,
            readonly: false,
            static_member: false,
            accessibility: None,
            r#abstract: false,
            value: None,
            signature: Some(signature),
        })
        .expect("index member");
    let slots = DeclarationSlots {
        value: true,
        ..TYPE_ONLY
    };
    let class = fixture.structural_declaration(
        "IndexClass",
        DeclarationPayload::Class {
            type_parameters: Box::new([]),
            members: Box::new([index]),
            heritage: Box::new([]),
        },
        slots,
        20,
        &[fixture.site(110, 150)],
    );
    let group = fixture.group("IndexClass", slots, vec![class]);
    fixture
        .assembly
        .add_value(ValueAttachmentRecipe {
            group,
            declaration: class,
        })
        .expect("class value attachment");
    let product = fixture.finish("class-index-full-walk");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[group.index()],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert!(receipt.unavailable.contains(&group.index()));
    assert_eq!(receipt.library_records.len(), 2);
}

fn assert_cross_fragment_overload_atomic(reverse: bool) {
    let mut fixture = Fixture::new();
    let first_type = fixture.keyword(
        if reverse {
            KeywordKind::This
        } else {
            KeywordKind::Number
        },
        102,
    );
    let second_type = fixture.keyword(
        if reverse {
            KeywordKind::Number
        } else {
            KeywordKind::This
        },
        242,
    );
    let first_overload = fixture.method("overload", first_type, 100);
    let second_overload = fixture.method("overload", second_type, 240);
    let sibling = fixture.property("keep", if reverse { second_type } else { first_type }, 260);
    let first_members = vec![first_overload];
    let first = fixture.structural_declaration(
        "Redeclared",
        DeclarationPayload::Interface {
            type_parameters: Box::new([]),
            members: first_members.into_boxed_slice(),
            heritage: Box::new([]),
        },
        TYPE_ONLY,
        20,
        &[fixture.site(100, 108)],
    );
    let second_members = vec![second_overload, sibling];
    let second = fixture.structural_declaration(
        "Redeclared",
        DeclarationPayload::Interface {
            type_parameters: Box::new([]),
            members: second_members.into_boxed_slice(),
            heritage: Box::new([]),
        },
        TYPE_ONLY,
        200,
        &[fixture.site(240, 248), fixture.site(260, 268)],
    );
    let group = fixture.group("Redeclared", TYPE_ONLY, vec![first, second]);
    let product = fixture.finish("cross-fragment-overload");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[group.index()],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert!(receipt.ready.contains(&group.index()));
    let ty = receipt.group_types[group.index()].expect("ready interface");
    let object = interner.store().object_type(ty).expect("interface object");
    assert_eq!(
        object
            .properties
            .iter()
            .filter_map(|property| property.key.as_string())
            .collect::<Vec<_>>(),
        vec!["keep"]
    );
}

#[test]
fn cross_fragment_overload_poison_is_atomic_forward() {
    assert_cross_fragment_overload_atomic(false);
}

#[test]
fn cross_fragment_overload_poison_is_atomic_reverse() {
    assert_cross_fragment_overload_atomic(true);
}

#[test]
fn malformed_no_infer_marker_is_typed_unavailable_not_ready_error() {
    let mut fixture = Fixture::new();
    let intrinsic = fixture.keyword(KeywordKind::Intrinsic, 42);
    let declaration = fixture.structural_declaration(
        "NoInfer",
        DeclarationPayload::TypeAlias {
            type_parameters: Box::new([]),
            value: intrinsic,
        },
        TYPE_ONLY,
        20,
        &[],
    );
    fixture
        .assembly
        .add_intrinsic_marker(IntrinsicMarkerRecipe {
            declaration,
            marker: IntrinsicMarker::NoInfer,
        });
    let group = fixture.group("NoInfer", TYPE_ONLY, vec![declaration]);
    let product = fixture.finish("malformed-no-infer-marker");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[group.index()],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert!(receipt.unavailable.contains(&group.index()));
    assert_eq!(receipt.group_types[group.index()], None);
    assert_eq!(receipt.library_records.len(), 1);
    let record = &receipt.library_records[0];
    assert_eq!(record.owner_start, 21);
    let crate::check::checker::project_local::project_local_pass_port::NormalizedCheckerRecordForTest::Incomplete(incomplete) = &record.record else {
        panic!("malformed NoInfer stages an incomplete")
    };
    assert_eq!(incomplete.span.start, 20);
    assert_eq!(incomplete.span.end, 200);
    assert_eq!(
        incomplete.id,
        "annotation-lower/intrinsic-marker/no-infer-shape"
    );
}

#[test]
fn single_parameter_no_infer_marker_remains_ready() {
    let mut fixture = Fixture::new();
    let intrinsic = fixture.keyword(KeywordKind::Intrinsic, 42);
    let declaration_site = fixture.site(20, 180);
    let parameter_name = fixture.assembly.intern("T").expect("parameter name");
    let parameter = fixture
        .assembly
        .add_type_parameter(TypeParameterRecipe {
            site: fixture.site(30, 31),
            owner: declaration_site,
            visibility: Box::new([declaration_site]),
            name: parameter_name,
            variance: Variance::Invariant,
            constant: false,
            constraint: None,
            default: None,
        })
        .expect("parameter");
    let declaration = fixture.structural_declaration(
        "NoInfer",
        DeclarationPayload::TypeAlias {
            type_parameters: Box::new([parameter]),
            value: intrinsic,
        },
        TYPE_ONLY,
        20,
        &[],
    );
    fixture
        .assembly
        .add_intrinsic_marker(IntrinsicMarkerRecipe {
            declaration,
            marker: IntrinsicMarker::NoInfer,
        });
    let group = fixture.group("NoInfer", TYPE_ONLY, vec![declaration]);
    let product = fixture.finish("single-parameter-no-infer-marker");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[group.index()],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert!(receipt.ready.contains(&group.index()));
    assert!(!receipt.unavailable.contains(&group.index()));
    assert!(receipt.group_types[group.index()].is_some());
    assert!(receipt.library_records.is_empty());
}

#[test]
fn multi_parameter_no_infer_marker_is_typed_unavailable() {
    let mut fixture = Fixture::new();
    let intrinsic = fixture.keyword(KeywordKind::Intrinsic, 42);
    let declaration_site = fixture.site(20, 180);
    let first_name = fixture.assembly.intern("T").expect("first parameter name");
    let second_name = fixture.assembly.intern("U").expect("second parameter name");
    let first = fixture
        .assembly
        .add_type_parameter(TypeParameterRecipe {
            site: fixture.site(30, 31),
            owner: declaration_site,
            visibility: Box::new([declaration_site]),
            name: first_name,
            variance: Variance::Invariant,
            constant: false,
            constraint: None,
            default: None,
        })
        .expect("first parameter");
    let second = fixture
        .assembly
        .add_type_parameter(TypeParameterRecipe {
            site: fixture.site(33, 34),
            owner: declaration_site,
            visibility: Box::new([declaration_site]),
            name: second_name,
            variance: Variance::Invariant,
            constant: false,
            constraint: None,
            default: None,
        })
        .expect("second parameter");
    let declaration = fixture.structural_declaration(
        "NoInfer",
        DeclarationPayload::TypeAlias {
            type_parameters: Box::new([first, second]),
            value: intrinsic,
        },
        TYPE_ONLY,
        20,
        &[],
    );
    fixture
        .assembly
        .add_intrinsic_marker(IntrinsicMarkerRecipe {
            declaration,
            marker: IntrinsicMarker::NoInfer,
        });
    let group = fixture.group("NoInfer", TYPE_ONLY, vec![declaration]);
    let product = fixture.finish("multi-parameter-no-infer-marker");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[group.index()],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert!(receipt.unavailable.contains(&group.index()));
    assert!(!receipt.ready.contains(&group.index()));
    assert_eq!(receipt.group_types[group.index()], None);
    assert_eq!(receipt.library_records.len(), 1);
    assert_eq!(receipt.library_records[0].owner_start, 21);
    let crate::check::checker::project_local::project_local_pass_port::NormalizedCheckerRecordForTest::Incomplete(incomplete) = &receipt.library_records[0].record else {
        panic!("multi-parameter NoInfer stages an incomplete")
    };
    assert_eq!(incomplete.span.start, 20);
    assert_eq!(incomplete.span.end, 200);
    assert_eq!(
        incomplete.id,
        "annotation-lower/intrinsic-marker/no-infer-shape"
    );
}

#[test]
fn unsupported_heritage_record_uses_the_heritage_site_as_owner() {
    let mut fixture = Fixture::new();
    let group = fixture.assembly.next_group_key().expect("interface group");
    let global = fixture.assembly.intern("globalThis").expect("global name");
    let heritage_site = fixture.site(72, 89);
    let heritage = fixture
        .assembly
        .add_type_recipe(TypeRecipe {
            site: heritage_site,
            node: TypeRecipeNode::Query {
                path: Box::new([global]),
                resolution: ValueReferenceResolution::IntrinsicGlobalValue,
                dependency: DependencyLinkRecipe {
                    owner: group,
                    kind: DependencyKind::Query,
                },
            },
        })
        .expect("heritage query");
    fixture.assembly.add_dependency(DependencyRecipe {
        owner: group,
        target: DependencyTarget::IntrinsicGlobalValue,
        kind: DependencyKind::Query,
        site: heritage_site,
    });
    let declaration = fixture.structural_declaration(
        "HeritageOwner",
        DeclarationPayload::Interface {
            type_parameters: Box::new([]),
            members: Box::new([]),
            heritage: Box::new([heritage]),
        },
        TYPE_ONLY,
        20,
        &[],
    );
    let actual_group = fixture.group("HeritageOwner", TYPE_ONLY, vec![declaration]);
    assert_eq!(actual_group, group);
    let product = fixture.finish("authenticated-heritage-owner");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[group.index()],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert!(receipt.unavailable.contains(&group.index()));
    assert!(!receipt.ready.contains(&group.index()));
    assert_eq!(receipt.group_types[group.index()], None);
    assert_eq!(receipt.library_records.len(), 1);
    assert_eq!(receipt.library_records[0].owner_start, heritage_site.start);
}

#[test]
fn unsupported_class_heritage_is_atomic_and_uses_the_heritage_site_as_owner() {
    let mut fixture = Fixture::new();
    let group = fixture.assembly.next_group_key().expect("class group");
    let global = fixture.assembly.intern("globalThis").expect("global name");
    let heritage_site = fixture.site(72, 89);
    let second_heritage_site = fixture.site(92, 109);
    let heritage = fixture
        .assembly
        .add_type_recipe(TypeRecipe {
            site: heritage_site,
            node: TypeRecipeNode::Query {
                path: Box::new([global]),
                resolution: ValueReferenceResolution::IntrinsicGlobalValue,
                dependency: DependencyLinkRecipe {
                    owner: group,
                    kind: DependencyKind::Query,
                },
            },
        })
        .expect("heritage query");
    let second_heritage = fixture
        .assembly
        .add_type_recipe(TypeRecipe {
            site: second_heritage_site,
            node: TypeRecipeNode::Query {
                path: Box::new([global]),
                resolution: ValueReferenceResolution::IntrinsicGlobalValue,
                dependency: DependencyLinkRecipe {
                    owner: group,
                    kind: DependencyKind::Query,
                },
            },
        })
        .expect("second heritage query");
    fixture.assembly.add_dependency(DependencyRecipe {
        owner: group,
        target: DependencyTarget::IntrinsicGlobalValue,
        kind: DependencyKind::Query,
        site: heritage_site,
    });
    fixture.assembly.add_dependency(DependencyRecipe {
        owner: group,
        target: DependencyTarget::IntrinsicGlobalValue,
        kind: DependencyKind::Query,
        site: second_heritage_site,
    });
    let unsupported_member_type = fixture.keyword(KeywordKind::This, 42);
    let unsupported_member = fixture.property("member", unsupported_member_type, 40);
    let slots = DeclarationSlots {
        value: true,
        ..TYPE_ONLY
    };
    let declaration = fixture.structural_declaration(
        "HeritageClass",
        DeclarationPayload::Class {
            type_parameters: Box::new([]),
            members: Box::new([unsupported_member]),
            heritage: Box::new([heritage, second_heritage]),
        },
        slots,
        20,
        &[fixture.site(40, 48)],
    );
    let actual_group = fixture.group("HeritageClass", slots, vec![declaration]);
    assert_eq!(actual_group, group);
    fixture
        .assembly
        .add_value(ValueAttachmentRecipe { group, declaration })
        .expect("class value attachment");
    let product = fixture.finish("atomic-class-heritage-owner");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[group.index()],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert!(receipt.unavailable.contains(&group.index()));
    assert!(!receipt.ready.contains(&group.index()));
    assert_eq!(receipt.group_types[group.index()], None);
    assert_eq!(receipt.library_records.len(), 3);
    assert_eq!(
        receipt
            .library_records
            .iter()
            .map(|record| record.owner_start)
            .collect::<Vec<_>>(),
        vec![40, heritage_site.start, second_heritage_site.start]
    );
}

#[test]
fn source_structural_scc_failure_makes_every_member_unavailable() {
    let source = r#"
        type Broken = typeof globalThis;
        interface A { b: B; }
        interface B extends Broken { a: A; }
    "#;
    let product = extract_library_product(source, "source-structural-scc-failure");
    let broken = product_group(&product, "Broken");
    let a = product_group(&product, "A");
    let b = product_group(&product, "B");
    assert!(product.dependency_sccs().iter().any(|component| {
        component.len() == 2
            && component.iter().any(|group| group.index() == a)
            && component.iter().any(|group| group.index() == b)
    }));
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[a, b, broken],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    for group in [a, b] {
        assert!(receipt.unavailable.contains(&group));
        assert!(!receipt.ready.contains(&group));
        assert_eq!(receipt.group_types[group], None);
    }
    assert!(receipt.unavailable.contains(&broken));
    assert_eq!(receipt.library_records.len(), 1);
}

#[test]
fn source_labeled_required_tuple_is_transparently_supported() {
    let source = "type LabeledTuple = [first: number, second: string];";
    let product = extract_library_product(source, "source-labeled-required-tuple");
    let group = product_group(&product, "LabeledTuple");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[group],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert!(receipt.ready.contains(&group));
    assert!(!receipt.unavailable.contains(&group));
    assert!(receipt.group_types[group].is_some());
    assert!(receipt.library_records.is_empty());
}

#[test]
fn source_optional_tuple_stages_exact_element_and_walks_unsupported_child() {
    let source = "type OptionalTuple = [head?: typeof globalThis];";
    let product = extract_library_product(source, "source-optional-tuple");
    let group = product_group(&product, "OptionalTuple");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[group],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert!(receipt.unavailable.contains(&group));
    assert!(!receipt.ready.contains(&group));
    assert_eq!(receipt.group_types[group], None);
    let incompletes = receipt
        .library_records
        .iter()
        .map(|record| match &record.record {
            crate::check::checker::project_local::project_local_pass_port::NormalizedCheckerRecordForTest::Incomplete(incomplete) => incomplete,
            crate::check::checker::project_local::project_local_pass_port::NormalizedCheckerRecordForTest::Diagnostic(_) => panic!("tuple lowering stages only incompletes"),
        })
        .collect::<Vec<_>>();
    assert_eq!(incompletes.len(), 2);
    assert!(!incompletes
        .iter()
        .any(|record| record.id == "annotation-lower/tuple-type/unsupported"));
    let optional = incompletes
        .iter()
        .find(|record| record.id == "annotation-lower/tuple-optional-element/self")
        .expect("typed optional-element incomplete");
    let element = "head?: typeof globalThis";
    let element_start =
        u32::try_from(source.find(element).expect("optional element")).expect("element offset");
    assert_eq!(optional.span.start, element_start);
    assert_eq!(optional.span.end, element_start + element.len() as u32);
    let child = incompletes
        .iter()
        .find(|record| record.id == "annotation-lower/type-query/typeof")
        .expect("unsupported child was visited");
    let child_text = "typeof globalThis";
    let child_start =
        u32::try_from(source.find(child_text).expect("child recipe")).expect("child offset");
    assert_eq!(child.span.start, child_start);
    assert_eq!(child.span.end, child_start + child_text.len() as u32);
}

#[test]
fn source_non_array_tuple_rest_stages_exact_element_and_walks_unsupported_child() {
    let source = "type RestTuple = [...tail: typeof globalThis];";
    let product = extract_library_product(source, "source-non-array-tuple-rest");
    let group = product_group(&product, "RestTuple");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[group],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert!(receipt.unavailable.contains(&group));
    assert!(!receipt.ready.contains(&group));
    assert_eq!(receipt.group_types[group], None);
    let incompletes = receipt
        .library_records
        .iter()
        .map(|record| match &record.record {
            crate::check::checker::project_local::project_local_pass_port::NormalizedCheckerRecordForTest::Incomplete(incomplete) => incomplete,
            crate::check::checker::project_local::project_local_pass_port::NormalizedCheckerRecordForTest::Diagnostic(_) => panic!("tuple lowering stages only incompletes"),
        })
        .collect::<Vec<_>>();
    assert_eq!(incompletes.len(), 2);
    assert!(!incompletes
        .iter()
        .any(|record| record.id == "annotation-lower/tuple-type/unsupported"));
    let rest = incompletes
        .iter()
        .find(|record| record.id == "annotation-lower/tuple-rest-element/non-array")
        .expect("typed rest-element incomplete");
    let element = "...tail: typeof globalThis";
    let element_start =
        u32::try_from(source.find(element).expect("rest element")).expect("element offset");
    assert_eq!(rest.span.start, element_start);
    assert_eq!(rest.span.end, element_start + element.len() as u32);
    let child = incompletes
        .iter()
        .find(|record| record.id == "annotation-lower/type-query/typeof")
        .expect("unsupported child was visited");
    let child_text = "typeof globalThis";
    let child_start =
        u32::try_from(source.find(child_text).expect("child recipe")).expect("child offset");
    assert_eq!(child.span.start, child_start);
    assert_eq!(child.span.end, child_start + child_text.len() as u32);
}

#[test]
fn source_array_tuple_rest_remains_supported() {
    let source = "type RestTuple = [...tail: number[]];";
    let product = extract_library_product(source, "source-array-tuple-rest");
    let group = product_group(&product, "RestTuple");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[group],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert!(receipt.ready.contains(&group));
    assert!(!receipt.unavailable.contains(&group));
    assert!(receipt.group_types[group].is_some());
    assert!(receipt.library_records.is_empty());
}

fn assert_source_type_parameter_rest_is_supported(source: &str, profile: &str) {
    let product = extract_library_product(source, profile);
    let group = product_group(&product, "Spread");
    let declaration = product.groups()[group].fragments[0];
    let DeclarationPayload::TypeAlias {
        type_parameters,
        value,
    } = &product.declarations()[declaration.index()].payload
    else {
        panic!("Spread is a type alias")
    };
    let [parameter] = type_parameters.as_ref() else {
        panic!("Spread has one type parameter")
    };
    assert!(product.type_parameters()[parameter.index()]
        .constraint
        .is_some());
    let TypeRecipeNode::Tuple { elements } = &product.types()[value.index()].node else {
        panic!("Spread body is a tuple")
    };
    let [element] = elements.as_ref() else {
        panic!("Spread tuple has one element")
    };
    assert!(element.rest);
    let TypeRecipeNode::Reference { resolution, .. } = &product.types()[element.value.index()].node
    else {
        panic!("tuple rest value is a product reference")
    };
    let TypeReferenceResolution::TypeParameter(candidate) = resolution else {
        panic!("tuple rest reference resolves to its type parameter")
    };
    assert_eq!(candidate, parameter);
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[group],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert!(receipt.ready.contains(&group));
    assert!(!receipt.unavailable.contains(&group));
    let template = receipt.group_types[group].expect("ready alias template");
    assert!(receipt.library_records.is_empty());
    let alias = interner
        .store()
        .alias_template_id(template)
        .expect("published alias identity");
    let PublishedAliasTerminal::Ready(surface) = receipt
        .aliases
        .terminal(alias)
        .expect("published alias terminal")
    else {
        panic!("constrained tuple-rest alias is semantically ready")
    };
    let parameter_id = surface.type_params()[0];
    assert!(interner
        .store()
        .type_param_constraint(parameter_id)
        .is_some());
    let parameter_ty = interner.intern_type_param(
        parameter_id,
        product.strings()[product.type_parameters()[parameter.index()].name.index()].clone(),
    );
    let tuple = interner
        .store()
        .tuple_type(surface.body())
        .expect("published alias body is a tuple");
    let rest = tuple
        .rest
        .as_ref()
        .expect("published tuple retains its rest");
    assert_eq!(rest.ty, parameter_ty);
}

#[test]
fn source_array_constrained_type_parameter_tuple_rest_is_supported() {
    assert_source_type_parameter_rest_is_supported(
        "type Spread<T extends number[]> = [...T];",
        "source-array-constrained-type-parameter-rest",
    );
}

#[test]
fn source_readonly_array_constrained_type_parameter_tuple_rest_is_supported() {
    assert_source_type_parameter_rest_is_supported(
        "type Spread<T extends readonly number[]> = [...T];",
        "source-readonly-array-constrained-type-parameter-rest",
    );
}

#[test]
fn source_tuple_constrained_type_parameter_tuple_rest_is_supported() {
    assert_source_type_parameter_rest_is_supported(
        "type Spread<T extends [number, string]> = [...T];",
        "source-tuple-constrained-type-parameter-rest",
    );
}

#[test]
fn source_unconstrained_infer_tuple_rest_is_supported() {
    let source = "type InferRest<T> = T extends [...infer R] ? R : never;";
    let product = extract_library_product(source, "source-unconstrained-infer-rest");
    let group = product_group(&product, "InferRest");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[group],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert!(receipt.ready.contains(&group));
    assert!(!receipt.unavailable.contains(&group));
    assert!(receipt.group_types[group].is_some());
    assert!(receipt.library_records.is_empty());
}

#[test]
fn source_constrained_infer_tuple_rest_stages_only_exact_infer_record() {
    let source = "type InferRest<T> = T extends [...infer R extends number[]] ? R : never;";
    let product = extract_library_product(source, "source-constrained-infer-rest");
    let group = product_group(&product, "InferRest");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[group],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert!(receipt.unavailable.contains(&group));
    assert!(!receipt.ready.contains(&group));
    assert_eq!(receipt.group_types[group], None);
    assert_eq!(receipt.library_records.len(), 1);
    let crate::check::checker::project_local::project_local_pass_port::NormalizedCheckerRecordForTest::Incomplete(incomplete) = &receipt.library_records[0].record else {
        panic!("constrained infer stages an incomplete")
    };
    assert_eq!(incomplete.id, "annotation-lower/infer-type/constraint");
    let infer = "infer R extends number[]";
    let start = u32::try_from(source.find(infer).expect("infer recipe")).expect("infer offset");
    assert_eq!(incomplete.span.start, start);
    assert_eq!(incomplete.span.end, start + infer.len() as u32);
}

#[test]
fn source_constrained_infer_tuple_rest_walks_nested_constraint_failure() {
    let source =
        "type InferRest<T> = T extends [...infer R extends typeof globalThis] ? R : never;";
    let product = extract_library_product(source, "source-nested-constrained-infer-rest");
    let group = product_group(&product, "InferRest");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[group],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert!(receipt.unavailable.contains(&group));
    assert!(!receipt.ready.contains(&group));
    assert_eq!(receipt.group_types[group], None);
    let incompletes = receipt
        .library_records
        .iter()
        .map(|record| match &record.record {
            crate::check::checker::project_local::project_local_pass_port::NormalizedCheckerRecordForTest::Incomplete(incomplete) => incomplete,
            crate::check::checker::project_local::project_local_pass_port::NormalizedCheckerRecordForTest::Diagnostic(_) => panic!("infer lowering stages only incompletes"),
        })
        .collect::<Vec<_>>();
    assert_eq!(incompletes.len(), 2);
    assert!(incompletes
        .iter()
        .any(|record| record.id == "annotation-lower/infer-type/constraint"));
    assert!(incompletes
        .iter()
        .any(|record| record.id == "annotation-lower/type-query/typeof"));
    assert!(!incompletes
        .iter()
        .any(|record| record.id == "annotation-lower/tuple-rest-element/non-array"));
}

#[test]
fn retry_of_the_same_record_producer_is_deduplicated_by_identity() {
    let mut fixture = Fixture::new();
    let this = fixture.keyword(KeywordKind::This, 122);
    let unsupported = fixture.method("unsupported", this, 110);
    let declaration = fixture.structural_declaration(
        "RetrySurface",
        DeclarationPayload::Interface {
            type_parameters: Box::new([]),
            members: Box::new([unsupported]),
            heritage: Box::new([]),
        },
        TYPE_ONLY,
        20,
        &[fixture.site(110, 130)],
    );
    let group = fixture.group("RetrySurface", TYPE_ONLY, vec![declaration]);
    let product = fixture.finish("record-retry-dedupe");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[group.index()],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    let records = accumulate_product_library_records_for_test(&[&receipt, &receipt]);
    assert_eq!(records.len(), 1);
}

#[test]
fn distinct_record_producers_preserve_identical_normalized_rows() {
    let mut fixture = Fixture::new();
    let first = fixture.keyword(KeywordKind::This, 122);
    let second = fixture.keyword(KeywordKind::This, 122);
    let parameter_name = fixture.assembly.intern("value").expect("parameter name");
    let signature = fixture
        .assembly
        .add_signature(SignatureRecipe {
            type_parameters: Box::new([]),
            parameters: Box::new([ParameterRecipe {
                pattern: typokat_library_product::ParameterPattern::Identifier(parameter_name),
                value: Some(first),
                optional: false,
                rest: false,
                this_parameter: false,
            }]),
            result: Some(second),
        })
        .expect("signature");
    let method_name = fixture.assembly.intern("duplicate").expect("method name");
    let method = fixture
        .assembly
        .add_member(MemberRecipe {
            site: fixture.site(110, 150),
            kind: MemberKind::Method,
            name: Some(PropertyNameRecipe::Identifier(method_name)),
            optional: false,
            readonly: false,
            static_member: false,
            accessibility: None,
            r#abstract: false,
            value: None,
            signature: Some(signature),
        })
        .expect("method");
    let declaration = fixture.structural_declaration(
        "DistinctProducers",
        DeclarationPayload::Interface {
            type_parameters: Box::new([]),
            members: Box::new([method]),
            heritage: Box::new([]),
        },
        TYPE_ONLY,
        20,
        &[fixture.site(110, 150)],
    );
    let group = fixture.group("DistinctProducers", TYPE_ONLY, vec![declaration]);
    let product = fixture.finish("distinct-record-producers");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[group.index()],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert_eq!(receipt.library_records.len(), 2);
    assert_eq!(receipt.library_records[0], receipt.library_records[1]);
}

#[test]
fn class_interface_group_does_not_publish_partial_interface_surface() {
    let mut fixture = Fixture::new();
    let number = fixture.keyword(KeywordKind::Number, 102);
    let class_member = fixture.property("keep", number, 100);
    let this = fixture.keyword(KeywordKind::This, 242);
    let deferred = fixture.method("sort", this, 240);
    let class_slots = DeclarationSlots {
        value: true,
        ..TYPE_ONLY
    };
    let class = fixture.structural_declaration(
        "Merged",
        DeclarationPayload::Class {
            type_parameters: Box::new([]),
            members: Box::new([class_member]),
            heritage: Box::new([]),
        },
        class_slots,
        20,
        &[fixture.site(100, 108)],
    );
    let interface = fixture.structural_declaration(
        "Merged",
        DeclarationPayload::Interface {
            type_parameters: Box::new([]),
            members: Box::new([deferred]),
            heritage: Box::new([]),
        },
        TYPE_ONLY,
        200,
        &[fixture.site(240, 248)],
    );
    let group = fixture.group("Merged", class_slots, vec![class, interface]);
    fixture
        .assembly
        .add_value(ValueAttachmentRecipe {
            group,
            declaration: class,
        })
        .expect("class value attachment");
    let product = fixture.finish("class-interface-atomicity");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[group.index()],
        &BTreeMap::new(),
        [None, None],
        None,
    );
    assert!(receipt.unavailable.contains(&group.index()));
    assert!(!receipt.ready.contains(&group.index()));
    assert_eq!(receipt.group_types[group.index()], None);
}

#[test]
fn missing_admission_from_member_dependency_is_not_deferred() {
    let mut fixture = Fixture::new();
    let target_declaration = fixture.structural_declaration(
        "Target",
        DeclarationPayload::Interface {
            type_parameters: Box::new([]),
            members: Box::new([]),
            heritage: Box::new([]),
        },
        TYPE_ONLY,
        20,
        &[],
    );
    let target = fixture.group("Target", TYPE_ONLY, vec![target_declaration]);
    let owner = fixture
        .assembly
        .next_group_key()
        .expect("dependent group key");
    let target_name = fixture.assembly.intern("Target").expect("target name");
    let reference_site = fixture.site(102, 106);
    let reference = fixture
        .assembly
        .add_type_recipe(TypeRecipe {
            site: reference_site,
            node: TypeRecipeNode::Reference {
                path: Box::new([target_name]),
                resolution: TypeReferenceResolution::Group(target),
                arguments: Box::new([]),
                dependency: DependencyLinkRecipe {
                    owner,
                    kind: DependencyKind::Reference,
                },
            },
        })
        .expect("target reference");
    let property = fixture.property("value", reference, 100);
    let owner_declaration = fixture.structural_declaration(
        "Owner",
        DeclarationPayload::Interface {
            type_parameters: Box::new([]),
            members: Box::new([property]),
            heritage: Box::new([]),
        },
        TYPE_ONLY,
        60,
        &[fixture.site(100, 108)],
    );
    let actual_owner = fixture.group("Owner", TYPE_ONLY, vec![owner_declaration]);
    assert_eq!(actual_owner, owner);
    fixture.assembly.add_dependency(DependencyRecipe {
        owner,
        target: DependencyTarget::Group(target),
        kind: DependencyKind::Reference,
        site: reference_site,
    });
    let product = fixture.finish("missing-admission");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[owner.index()],
        &BTreeMap::new(),
        [None, None],
        None,
    );
    assert_eq!(receipt.missing_admissions, BTreeSet::from([target.index()]));
    assert!(!receipt.ready.contains(&owner.index()));
}

#[test]
fn conditional_alias_branches_are_productive_but_the_check_stays_direct() {
    let mut fixture = Fixture::new();
    let recursive_group = fixture
        .assembly
        .next_group_key()
        .expect("conditional alias group key");
    let declaration_site = fixture.site(20, 200);
    let parameter_name = fixture.assembly.intern("T").expect("parameter name");
    let parameter = fixture
        .assembly
        .add_type_parameter(TypeParameterRecipe {
            site: fixture.site(32, 33),
            owner: declaration_site,
            visibility: Box::new([declaration_site]),
            name: parameter_name,
            variance: Variance::Invariant,
            constant: false,
            constraint: None,
            default: None,
        })
        .expect("type parameter");
    let check_site = fixture.site(52, 53);
    let check = fixture
        .assembly
        .add_type_recipe(TypeRecipe {
            site: check_site,
            node: TypeRecipeNode::Reference {
                path: Box::new([parameter_name]),
                resolution: TypeReferenceResolution::TypeParameter(parameter),
                arguments: Box::new([]),
                dependency: DependencyLinkRecipe {
                    owner: recursive_group,
                    kind: DependencyKind::Reference,
                },
            },
        })
        .expect("conditional check");
    fixture.assembly.add_dependency(DependencyRecipe {
        owner: recursive_group,
        target: DependencyTarget::TypeParameter(parameter),
        kind: DependencyKind::Reference,
        site: check_site,
    });
    let string = fixture.keyword(KeywordKind::String, 70);
    let value_member = fixture.property("value", string, 80);
    let extends = fixture
        .assembly
        .add_type_recipe(TypeRecipe {
            site: fixture.site(76, 96),
            node: TypeRecipeNode::Object(Box::new([value_member])),
        })
        .expect("conditional extends object");
    let recursive_name = fixture
        .assembly
        .intern("ConditionalRecursive")
        .expect("recursive alias name");
    let recursive_site = fixture.site(112, 132);
    let recursive_branch = fixture
        .assembly
        .add_type_recipe(TypeRecipe {
            site: recursive_site,
            node: TypeRecipeNode::Reference {
                path: Box::new([recursive_name]),
                resolution: TypeReferenceResolution::Group(recursive_group),
                arguments: Box::new([string]),
                dependency: DependencyLinkRecipe {
                    owner: recursive_group,
                    kind: DependencyKind::Reference,
                },
            },
        })
        .expect("productive recursive branch");
    fixture.assembly.add_dependency(DependencyRecipe {
        owner: recursive_group,
        target: DependencyTarget::Group(recursive_group),
        kind: DependencyKind::Reference,
        site: recursive_site,
    });
    let conditional = fixture
        .assembly
        .add_type_recipe(TypeRecipe {
            site: fixture.site(48, 150),
            node: TypeRecipeNode::Conditional {
                check,
                extends,
                when_true: recursive_branch,
                when_false: check,
            },
        })
        .expect("conditional recipe");
    let recursive_declaration = fixture.structural_declaration(
        "ConditionalRecursive",
        DeclarationPayload::TypeAlias {
            type_parameters: Box::new([parameter]),
            value: conditional,
        },
        TYPE_ONLY,
        20,
        &[fixture.site(80, 88)],
    );
    let actual_recursive_group = fixture.group(
        "ConditionalRecursive",
        TYPE_ONLY,
        vec![recursive_declaration],
    );
    assert_eq!(actual_recursive_group, recursive_group);

    let direct_group = fixture
        .assembly
        .next_group_key()
        .expect("direct conditional group key");
    let direct_name = fixture
        .assembly
        .intern("DirectConditional")
        .expect("direct conditional name");
    let direct_site = fixture.site(252, 272);
    let direct_check = fixture
        .assembly
        .add_type_recipe(TypeRecipe {
            site: direct_site,
            node: TypeRecipeNode::Reference {
                path: Box::new([direct_name]),
                resolution: TypeReferenceResolution::Group(direct_group),
                arguments: Box::new([]),
                dependency: DependencyLinkRecipe {
                    owner: direct_group,
                    kind: DependencyKind::Reference,
                },
            },
        })
        .expect("direct conditional check");
    fixture.assembly.add_dependency(DependencyRecipe {
        owner: direct_group,
        target: DependencyTarget::Group(direct_group),
        kind: DependencyKind::Reference,
        site: direct_site,
    });
    let never = fixture.keyword(KeywordKind::Never, 290);
    let direct_conditional = fixture
        .assembly
        .add_type_recipe(TypeRecipe {
            site: fixture.site(248, 310),
            node: TypeRecipeNode::Conditional {
                check: direct_check,
                extends: string,
                when_true: string,
                when_false: never,
            },
        })
        .expect("direct conditional recipe");
    let direct_declaration = fixture.structural_declaration(
        "DirectConditional",
        DeclarationPayload::TypeAlias {
            type_parameters: Box::new([]),
            value: direct_conditional,
        },
        TYPE_ONLY,
        220,
        &[],
    );
    let actual_direct_group =
        fixture.group("DirectConditional", TYPE_ONLY, vec![direct_declaration]);
    assert_eq!(actual_direct_group, direct_group);

    let product = fixture.finish("conditional-recursive-alias");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[recursive_group.index(), direct_group.index()],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert!(receipt.ready.contains(&recursive_group.index()));
    assert!(receipt.unavailable.contains(&direct_group.index()));
    let template = receipt.group_types[recursive_group.index()].expect("ready alias template");
    let alias = interner
        .store()
        .alias_template_id(template)
        .expect("alias identity");
    let PublishedAliasTerminal::Ready(surface) = receipt
        .aliases
        .terminal(alias)
        .expect("published recursive alias")
    else {
        panic!("productive conditional alias is ready")
    };
    let parameter = surface.type_params()[0];
    let string = interner.well_known().string;
    let input = interner.intern_object(ObjectType {
        properties: vec![PropertyType::public("value", string)],
        ..ObjectType::default()
    });
    let application = interner.intern_instantiation(template, vec![(parameter, input)]);
    let published = AliasPublication {
        classes: PublishedClasses::empty(),
        aliases: receipt.aliases,
    };
    let mut state = SemanticQueryState::default();
    let mut next_type_parameter = 1;
    assert!(matches!(
        SemanticQueryCoordinator::new(
            &mut interner,
            &published,
            &mut state,
            &mut next_type_parameter,
        )
        .is_assignable(application, string),
        RelationOutcome::Yes
    ));
}

#[test]
fn declared_method_signature_is_a_productive_alias_boundary() {
    let source = r#"
        type A = I | string;
        interface I {
            m(x: A): void;
            x: A;
            [k: string]: A;
        }
        type Direct = Direct;
    "#;
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::d_ts()).parse();
    let manifest = SourceDeclarationManifest::build(&parsed.program);
    let prelude_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
    let mut builder = ProjectBinderBuilder::new_empty_library_prelude(&prelude.program);
    let unit = CompilationUnit {
        source: SourceUnitKey(1),
        origin: CompilationOrigin::Library(LibraryFileOrdinal::new(0)),
        binding: ModuleBindingContext::for_program(
            &parsed.program,
            source_file_kind("declared-method-recursion.d.ts"),
        ),
    };
    let modules = builder
        .try_add_library_modules_with_declaration_manifests(&[(&parsed.program, &manifest, unit)])
        .expect("bind declared-method recursion product");
    let binder = builder.finish(modules[0]);
    let product = project_library_declaration_product(
        "declared-method-recursion",
        &binder,
        &[LibraryProductSource {
            ordinal: 0,
            name: "declared-method-recursion.d.ts",
            source_sha256: "00",
            byte_len: u32::try_from(source.len()).expect("source length"),
            external_module: false,
            module: modules[0],
            manifest: &manifest,
            source_text: source,
        }],
        &[],
    )
    .expect("extract declared-method recursion product");
    let group = |name: &str| {
        product
            .groups()
            .iter()
            .position(|group| product.strings()[group.name.index()] == name)
            .unwrap_or_else(|| panic!("missing {name} group"))
    };
    let alias_group = group("A");
    let interface_group = group("I");
    let direct_group = group("Direct");
    let materialized = vec![alias_group, interface_group, direct_group];
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &materialized,
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert!(receipt.ready.contains(&alias_group));
    assert!(receipt.ready.contains(&interface_group));
    assert!(receipt.unavailable.contains(&direct_group));
    let interface = receipt.group_types[interface_group].expect("ready interface surface");
    let interface = interner
        .store()
        .object_type(interface)
        .expect("interface is structural");
    let template = receipt.group_types[alias_group].expect("ready alias template");
    let alias = interner
        .store()
        .alias_template_id(template)
        .expect("published alias identity");
    let PublishedAliasTerminal::Ready(alias) = receipt
        .aliases
        .terminal(alias)
        .expect("published alias terminal")
    else {
        panic!("productive alias is ready")
    };
    assert_eq!(alias.template(), template);
    let assert_alias_application = |ty| {
        let application = interner
            .store()
            .instantiation_type(ty)
            .expect("member retains the alias application");
        assert_eq!(application.base, template);
        assert!(application.args.is_empty());
    };
    let method = interface
        .properties
        .iter()
        .find(|property| property.key.as_string() == Some("m"))
        .expect("method remains observable");
    let method = interner
        .store()
        .function_type(method.ty)
        .expect("method retains its function signature");
    assert_eq!(method.params.len(), 1);
    assert!(!method.params[0].optional);
    assert!(!method.params[0].has_default);
    assert!(!method.params[0].rest);
    assert_alias_application(method.params[0].ty);
    let property = interface
        .property("x")
        .expect("property remains observable");
    assert_alias_application(property.ty);
    assert_alias_application(
        interface
            .string_index
            .expect("string index remains observable"),
    );
}

#[test]
fn intrinsic_global_type_query_is_staged_and_its_interface_member_is_omitted() {
    let source = r#"
        interface Window {
            self: Window & typeof globalThis;
            keep: number;
        }
    "#;
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::d_ts()).parse();
    let manifest = SourceDeclarationManifest::build(&parsed.program);
    let prelude_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
    let mut builder = ProjectBinderBuilder::new_empty_library_prelude(&prelude.program);
    let unit = CompilationUnit {
        source: SourceUnitKey(1),
        origin: CompilationOrigin::Library(LibraryFileOrdinal::new(0)),
        binding: ModuleBindingContext::for_program(
            &parsed.program,
            source_file_kind("intrinsic-global-query.d.ts"),
        ),
    };
    let modules = builder
        .try_add_library_modules_with_declaration_manifests(&[(&parsed.program, &manifest, unit)])
        .expect("bind intrinsic-global query product");
    let binder = builder.finish(modules[0]);
    let product = project_library_declaration_product(
        "intrinsic-global-query",
        &binder,
        &[LibraryProductSource {
            ordinal: 0,
            name: "intrinsic-global-query.d.ts",
            source_sha256: "00",
            byte_len: u32::try_from(source.len()).expect("source length"),
            external_module: false,
            module: modules[0],
            manifest: &manifest,
            source_text: source,
        }],
        &[],
    )
    .expect("extract intrinsic-global query product");
    let window = product
        .groups()
        .iter()
        .position(|group| product.strings()[group.name.index()] == "Window")
        .expect("Window group");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[window],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert!(receipt.ready.contains(&window));
    let window = receipt.group_types[window].expect("ready Window surface");
    let window = interner
        .store()
        .object_type(window)
        .expect("Window is structural");
    assert_eq!(
        window
            .properties
            .iter()
            .filter_map(|property| property.key.as_string())
            .collect::<Vec<_>>(),
        vec!["keep"]
    );
    let incompletes = receipt
        .library_records
        .iter()
        .filter_map(|record| match &record.record {
            crate::check::checker::project_local::project_local_pass_port::NormalizedCheckerRecordForTest::Incomplete(incomplete) => Some(incomplete),
            crate::check::checker::project_local::project_local_pass_port::NormalizedCheckerRecordForTest::Diagnostic(_) => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(incompletes.len(), 1);
    assert_eq!(incompletes[0].id, "annotation-lower/type-query/typeof");
    assert_eq!(
        receipt.library_records[0].owner_start,
        u32::try_from(source.find("self:").expect("self member")).expect("member offset")
    );
    assert_eq!(
        &source[usize::try_from(incompletes[0].span.start).expect("start")
            ..usize::try_from(incompletes[0].span.end).expect("end")],
        "typeof globalThis"
    );
}

#[test]
fn object_recipe_guards_recursive_alias_but_direct_cycle_stays_unavailable() {
    let mut fixture = Fixture::new();

    let object_group = fixture.assembly.next_group_key().expect("object alias key");
    let object_name = fixture.assembly.intern("ObjectNode").expect("object name");
    let object_reference_site = fixture.site(102, 112);
    let object_reference = fixture
        .assembly
        .add_type_recipe(TypeRecipe {
            site: object_reference_site,
            node: TypeRecipeNode::Reference {
                path: Box::new([object_name]),
                resolution: TypeReferenceResolution::Group(object_group),
                arguments: Box::new([]),
                dependency: DependencyLinkRecipe {
                    owner: object_group,
                    kind: DependencyKind::Reference,
                },
            },
        })
        .expect("object self reference");
    let next = fixture.property("next", object_reference, 100);
    let object = fixture
        .assembly
        .add_type_recipe(TypeRecipe {
            site: fixture.site(96, 116),
            node: TypeRecipeNode::Object(Box::new([next])),
        })
        .expect("object recipe");
    let object_declaration = fixture.structural_declaration(
        "ObjectNode",
        DeclarationPayload::TypeAlias {
            type_parameters: Box::new([]),
            value: object,
        },
        TYPE_ONLY,
        20,
        &[fixture.site(100, 108)],
    );
    let actual_object_group = fixture.group("ObjectNode", TYPE_ONLY, vec![object_declaration]);
    assert_eq!(actual_object_group, object_group);
    fixture.assembly.add_dependency(DependencyRecipe {
        owner: object_group,
        target: DependencyTarget::Group(object_group),
        kind: DependencyKind::Reference,
        site: object_reference_site,
    });

    let direct_group = fixture.assembly.next_group_key().expect("direct alias key");
    let direct_name = fixture.assembly.intern("DirectLoop").expect("direct name");
    let direct_reference_site = fixture.site(302, 312);
    let direct_reference = fixture
        .assembly
        .add_type_recipe(TypeRecipe {
            site: direct_reference_site,
            node: TypeRecipeNode::Reference {
                path: Box::new([direct_name]),
                resolution: TypeReferenceResolution::Group(direct_group),
                arguments: Box::new([]),
                dependency: DependencyLinkRecipe {
                    owner: direct_group,
                    kind: DependencyKind::Reference,
                },
            },
        })
        .expect("direct self reference");
    let direct_declaration = fixture.structural_declaration(
        "DirectLoop",
        DeclarationPayload::TypeAlias {
            type_parameters: Box::new([]),
            value: direct_reference,
        },
        TYPE_ONLY,
        220,
        &[],
    );
    let actual_direct_group = fixture.group("DirectLoop", TYPE_ONLY, vec![direct_declaration]);
    assert_eq!(actual_direct_group, direct_group);
    fixture.assembly.add_dependency(DependencyRecipe {
        owner: direct_group,
        target: DependencyTarget::Group(direct_group),
        kind: DependencyKind::Reference,
        site: direct_reference_site,
    });

    let product = fixture.finish("recursive-object-alias");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[object_group.index(), direct_group.index()],
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert!(receipt.ready.contains(&object_group.index()));
    assert!(!receipt.unavailable.contains(&object_group.index()));
    assert!(receipt.unavailable.contains(&direct_group.index()));
    assert!(!receipt.ready.contains(&direct_group.index()));
    let unavailable = receipt
        .aliases
        .owned_terminals()
        .into_iter()
        .filter_map(|(_, terminal)| match terminal {
            PublishedAliasTerminal::Unavailable(reason) => Some(reason),
            PublishedAliasTerminal::Ready(_) => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(unavailable, vec![PublishedAliasUnavailable::DirectCycle]);
}

fn assert_mutual_direct_alias_cycle_reasons(reverse: bool) {
    let source = "type A = B; type B = A;";
    let product = extract_library_product(source, "mutual-direct-alias-cycle");
    let a = product_group(&product, "A");
    let b = product_group(&product, "B");
    let materialized = if reverse { vec![b, a] } else { vec![a, b] };
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &materialized,
        &BTreeMap::new(),
        [None, None],
        None,
    );

    assert_eq!(receipt.unavailable, BTreeSet::from([a, b]));
    assert_eq!(receipt.group_types[a], None);
    assert_eq!(receipt.group_types[b], None);
    let terminals = receipt.aliases.owned_terminals();
    assert_eq!(terminals.len(), 2);
    assert!(terminals.iter().all(|(_, terminal)| matches!(
        terminal,
        PublishedAliasTerminal::Unavailable(PublishedAliasUnavailable::DirectCycle)
    )));
}

#[test]
fn mutual_direct_alias_cycle_reasons_are_stable_forward() {
    assert_mutual_direct_alias_cycle_reasons(false);
}

#[test]
fn mutual_direct_alias_cycle_reasons_are_stable_reverse() {
    assert_mutual_direct_alias_cycle_reasons(true);
}

#[test]
fn force_all_source_census_deduplicates_visited_and_unvisited_deferred_records() {
    let mut fixture = Fixture::new();

    let default = fixture.keyword(KeywordKind::Intrinsic, 42);
    let parameter_name = fixture.assembly.intern("T").expect("parameter name");
    let parameter = fixture
        .assembly
        .add_type_parameter(TypeParameterRecipe {
            site: fixture.site(30, 31),
            owner: fixture.site(20, 180),
            visibility: Box::new([fixture.site(20, 180)]),
            name: parameter_name,
            variance: Variance::Invariant,
            constant: false,
            constraint: None,
            default: Some(default),
        })
        .expect("defaulted parameter");
    let target_declaration = fixture.structural_declaration(
        "Defaulted",
        DeclarationPayload::Interface {
            type_parameters: Box::new([parameter]),
            members: Box::new([]),
            heritage: Box::new([]),
        },
        TYPE_ONLY,
        20,
        &[],
    );
    let target = fixture.group("Defaulted", TYPE_ONLY, vec![target_declaration]);

    let owner = fixture.assembly.next_group_key().expect("owner group key");
    let nested_this = fixture.keyword(KeywordKind::This, 226);
    let visited_predicate = fixture
        .assembly
        .add_type_recipe(TypeRecipe {
            site: fixture.site(222, 230),
            node: TypeRecipeNode::Predicate {
                parameter: parameter_name,
                asserts: false,
                value: Some(nested_this),
            },
        })
        .expect("visited predicate");
    let visited_predicate_member = fixture.method("visitedPredicate", visited_predicate, 220);
    let number = fixture.keyword(KeywordKind::Number, 242);
    let visited_computed = fixture.computed_property(owner, "visitedUnique", number, 240);
    let blocker_type = fixture.keyword(KeywordKind::Intrinsic, 262);
    let blocker = fixture.property("blocker", blocker_type, 260);
    let unvisited_predicate = fixture
        .assembly
        .add_type_recipe(TypeRecipe {
            site: fixture.site(282, 290),
            node: TypeRecipeNode::Predicate {
                parameter: parameter_name,
                asserts: false,
                value: None,
            },
        })
        .expect("unvisited predicate");
    let unvisited_predicate_member = fixture.method("unvisitedPredicate", unvisited_predicate, 280);
    let unvisited_computed = fixture.computed_property(owner, "unvisitedUnique", number, 300);
    let target_name = fixture.assembly.intern("Defaulted").expect("target name");
    let application_site = fixture.site(322, 331);
    let defaulted_application = fixture
        .assembly
        .add_type_recipe(TypeRecipe {
            site: application_site,
            node: TypeRecipeNode::Reference {
                path: Box::new([target_name]),
                resolution: TypeReferenceResolution::Group(target),
                arguments: Box::new([]),
                dependency: DependencyLinkRecipe {
                    owner,
                    kind: DependencyKind::Reference,
                },
            },
        })
        .expect("defaulted application");
    let defaulted_member = fixture.property("defaulted", defaulted_application, 320);
    fixture.assembly.add_dependency(DependencyRecipe {
        owner,
        target: DependencyTarget::Group(target),
        kind: DependencyKind::Reference,
        site: application_site,
    });
    let owner_declaration = fixture.structural_declaration(
        "Owner",
        DeclarationPayload::Interface {
            type_parameters: Box::new([]),
            members: Box::new([
                visited_predicate_member,
                visited_computed,
                blocker,
                unvisited_predicate_member,
                unvisited_computed,
                defaulted_member,
            ]),
            heritage: Box::new([]),
        },
        TYPE_ONLY,
        200,
        &[
            fixture.site(220, 228),
            fixture.site(240, 248),
            fixture.site(260, 268),
            fixture.site(280, 288),
            fixture.site(300, 308),
            fixture.site(320, 328),
        ],
    );
    let actual_owner = fixture.group("Owner", TYPE_ONLY, vec![owner_declaration]);
    assert_eq!(actual_owner, owner);

    let product = fixture.finish("force-all-source-census");
    let mut interner = Interner::with_intrinsics();
    let receipt = lower_product_groups_for_test(
        &product,
        &mut interner,
        &[target.index(), owner.index()],
        &BTreeMap::new(),
        [None, None],
        None,
    );
    let incomplete_ids = receipt
        .library_records
        .iter()
        .filter_map(|record| match &record.record {
            crate::check::checker::project_local::project_local_pass_port::NormalizedCheckerRecordForTest::Incomplete(incomplete) => Some(incomplete.id.as_str()),
            crate::check::checker::project_local::project_local_pass_port::NormalizedCheckerRecordForTest::Diagnostic(_) => None,
        })
        .collect::<Vec<_>>();

    assert!(receipt.ready.contains(&owner.index()));
    assert_eq!(incomplete_ids.len(), 8);
    assert_eq!(
        incomplete_ids
            .iter()
            .filter(|id| **id == "annotation-lower/type-predicate/self")
            .count(),
        2
    );
    assert_eq!(
        incomplete_ids
            .iter()
            .filter(|id| **id == "signature/property-signature/computed-key")
            .count(),
        2
    );
    assert_eq!(
        incomplete_ids
            .iter()
            .filter(|id| **id == "annotation-lower/type-reference/default-argument")
            .count(),
        1
    );
    assert_eq!(
        incomplete_ids
            .iter()
            .filter(|id| **id == "annotation-lower/this-type/self")
            .count(),
        1,
        "the syntax walk continues through the unsupported predicate"
    );
    assert_eq!(
        incomplete_ids
            .iter()
            .filter(|id| **id == "annotation-lower/intrinsic-keyword/self")
            .count(),
        2
    );
}

use crate::check::checker::project_local::lowerer::lower_product_groups_for_test;
use crate::types::{Interner, PropertyKey};
use std::collections::{BTreeMap, BTreeSet};
use typokat_library_product::{
    ContainerKind, ContainerRecipe, DeclarationFlags, DeclarationGroupRecipe, DeclarationKey,
    DeclarationLexicalScope, DeclarationPayload, DeclarationRecipe, DeclarationSlots,
    DependencyKind, DependencyLinkRecipe, DependencyRecipe, DependencyTarget, GroupKey,
    KeywordKind, LexicalStepKind, LexicalStepRecipe, LibraryDeclarationProduct, MemberKey,
    MemberKind, MemberRecipe, ProductAssembly, PropertyNameRecipe, RegionKey, RegionKind,
    RegionRecipe, SignatureRecipe, SourceClassification, SourceKey, SourceRecipe, SourceSite,
    TypeParameterRecipe, TypeRecipe, TypeRecipeKey, TypeRecipeNode, TypeReferenceResolution,
    ValueAttachmentRecipe, ValueReferenceResolution, Variance,
};

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
        false,
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
        false,
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
        false,
        None,
    );
    assert_eq!(receipt.missing_admissions, BTreeSet::from([target.index()]));
    assert!(!receipt.ready.contains(&owner.index()));
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
        false,
        None,
    );

    assert!(receipt.ready.contains(&object_group.index()));
    assert!(!receipt.unavailable.contains(&object_group.index()));
    assert!(receipt.unavailable.contains(&direct_group.index()));
    assert!(!receipt.ready.contains(&direct_group.index()));
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
    let visited_predicate = fixture
        .assembly
        .add_type_recipe(TypeRecipe {
            site: fixture.site(222, 230),
            node: TypeRecipeNode::Predicate {
                parameter: parameter_name,
                asserts: false,
                value: None,
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
        true,
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
}

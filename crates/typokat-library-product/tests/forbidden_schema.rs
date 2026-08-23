use typokat_library_product::{
    ContainerKind, ContainerRecipe, DeclarationFlags, DeclarationGroupRecipe,
    DeclarationLexicalScope, DeclarationPayload, DeclarationRecipe, DeclarationSlots,
    DependencyKind, DependencyLinkRecipe, DependencyRecipe, DependencyTarget, LexicalStepKind,
    LexicalStepRecipe, MemberKind, MemberRecipe, ProductAssembly, ProductBuildError,
    ProductFieldClass, ProductFieldVisitor, ProductValidationError, RegionKind, RegionRecipe,
    SourceClassification, SourceRecipe, SourceSite, TypeParameterRecipe, TypeRecipe,
    TypeRecipeNode, TypeReferenceResolution, ValueAttachmentRecipe, Variance,
};

struct FieldCounter {
    fields: usize,
    owned_text: usize,
}

impl ProductFieldVisitor for FieldCounter {
    fn visit_field(
        &mut self,
        class: ProductFieldClass,
        _table: &'static str,
        _row: usize,
        _field: &'static str,
    ) {
        self.fields += 1;
        self.owned_text += usize::from(class == ProductFieldClass::OwnedText);
    }
}

#[test]
fn ownership_walker_has_a_typed_consumer() {
    let mut assembly = ProductAssembly::new();
    let container = assembly
        .add_container(ContainerRecipe {
            parent: None,
            kind: ContainerKind::CompilationGlobal,
            name: None,
            anchor: None,
        })
        .expect("container");
    let source = assembly.next_source_key().expect("source key");
    let root = assembly.next_region_key().expect("region key");
    let name = assembly.intern("lib.d.ts").expect("name");
    let sha256 = assembly.intern("00").expect("digest");
    assembly
        .add_source(SourceRecipe {
            ordinal: 0,
            name,
            sha256,
            byte_len: 0,
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
                end: 0,
            },
        })
        .expect("region");
    let product = assembly.finish("profile").expect("product");
    let mut counter = FieldCounter {
        fields: 0,
        owned_text: 0,
    };
    product.walk_fields(&mut counter);
    assert!(counter.fields > 0);
    assert!(counter.owned_text > 0);
}

#[test]
fn validation_rejects_a_missing_owned_region() {
    let mut assembly = ProductAssembly::new();
    assembly
        .add_container(ContainerRecipe {
            parent: None,
            kind: ContainerKind::CompilationGlobal,
            name: None,
            anchor: None,
        })
        .expect("container");
    let root = assembly.next_region_key().expect("region key");
    let name = assembly.intern("lib.d.ts").expect("name");
    let sha256 = assembly.intern("00").expect("digest");
    assembly
        .add_source(SourceRecipe {
            ordinal: 0,
            name,
            sha256,
            byte_len: 0,
            classification: SourceClassification::Script,
            root_region: root,
        })
        .expect("source");
    assert!(assembly.finish("profile").is_err());
}

fn valid_shell() -> (
    ProductAssembly,
    typokat_library_product::SourceKey,
    typokat_library_product::RegionKey,
    typokat_library_product::StringKey,
    typokat_library_product::ContainerKey,
) {
    let mut assembly = ProductAssembly::new();
    let container = assembly
        .add_container(ContainerRecipe {
            parent: None,
            kind: ContainerKind::CompilationGlobal,
            name: None,
            anchor: None,
        })
        .expect("container");
    let source = assembly.next_source_key().expect("source key");
    let root = assembly.next_region_key().expect("root key");
    let name = assembly.intern("x.d.ts").expect("name");
    let digest = assembly.intern("00").expect("digest");
    assembly
        .add_source(SourceRecipe {
            ordinal: 0,
            name,
            sha256: digest,
            byte_len: 10,
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
                end: 10,
            },
        })
        .expect("root");
    (assembly, source, root, name, container)
}

fn external_shell(anchor_end: u32) -> ProductAssembly {
    let mut assembly = ProductAssembly::new();
    let source = assembly.next_source_key().expect("source");
    let root = assembly.next_region_key().expect("root");
    let container = assembly
        .add_container(ContainerRecipe {
            parent: None,
            kind: ContainerKind::ExternalModule,
            name: None,
            anchor: Some(SourceSite {
                source,
                start: 0,
                end: anchor_end,
            }),
        })
        .expect("external container");
    let name = assembly.intern("module.d.ts").expect("name");
    let digest = assembly.intern("00").expect("digest");
    assembly
        .add_source(SourceRecipe {
            ordinal: 0,
            name,
            sha256: digest,
            byte_len: 10,
            classification: SourceClassification::ExternalModule,
            root_region: root,
        })
        .expect("source row");
    assembly
        .add_region(RegionRecipe {
            parent: None,
            container,
            kind: RegionKind::Source,
            site: SourceSite {
                source,
                start: 0,
                end: 10,
            },
        })
        .expect("root row");
    assembly
}

#[test]
fn typed_corruption_controls_cover_owned_links() {
    let (mut cyclic, source, _root, _, container) = valid_shell();
    let child = cyclic.next_region_key().expect("child key");
    cyclic
        .add_region(RegionRecipe {
            parent: Some(child),
            container,
            kind: RegionKind::Namespace,
            site: SourceSite {
                source,
                start: 1,
                end: 9,
            },
        })
        .expect("cyclic region row");
    assert!(cyclic.finish("profile").is_err());

    let (mut linked, source, root, name, _container) = valid_shell();
    let member = linked
        .add_member(MemberRecipe {
            site: SourceSite {
                source,
                start: 7,
                end: 8,
            },
            kind: MemberKind::Property,
            name: None,
            optional: false,
            readonly: false,
            static_member: false,
            accessibility: None,
            r#abstract: false,
            value: None,
            signature: None,
        })
        .expect("member");
    linked
        .add_declaration(DeclarationRecipe {
            site: SourceSite {
                source,
                start: 1,
                end: 5,
            },
            binding_site: SourceSite {
                source,
                start: 1,
                end: 2,
            },
            region: root,
            lexical_scope: DeclarationLexicalScope::None,
            name,
            spaces: DeclarationSlots {
                value: false,
                r#type: true,
                namespace: false,
            },
            flags: DeclarationFlags::default(),
            payload: DeclarationPayload::Interface {
                type_parameters: Box::new([]),
                members: Box::new([member]),
                heritage: Box::new([]),
            },
        })
        .expect("declaration");
    assert!(linked.finish("profile").is_err());

    let (mut attached, source, root, name, container) = valid_shell();
    let declaration = attached
        .add_declaration(DeclarationRecipe {
            site: SourceSite {
                source,
                start: 1,
                end: 3,
            },
            binding_site: SourceSite {
                source,
                start: 1,
                end: 2,
            },
            region: root,
            lexical_scope: DeclarationLexicalScope::None,
            name,
            spaces: DeclarationSlots {
                value: false,
                r#type: false,
                namespace: true,
            },
            flags: DeclarationFlags::default(),
            payload: DeclarationPayload::Namespace { child_region: root },
        })
        .expect("declaration");
    let group = attached
        .add_group(DeclarationGroupRecipe {
            container,
            name,
            slots: DeclarationSlots {
                namespace: true,
                ..DeclarationSlots::default()
            },
            fragments: Box::new([]),
        })
        .expect("group");
    attached
        .add_namespace(typokat_library_product::NamespaceAttachmentRecipe { group, declaration })
        .expect("attachment");
    assert!(attached.finish("profile").is_err());

    let (mut dependent, source, root, name, container) = valid_shell();
    let group = dependent
        .add_group(DeclarationGroupRecipe {
            container,
            name,
            slots: DeclarationSlots::default(),
            fragments: Box::new([]),
        })
        .expect("group");
    dependent.add_dependency(DependencyRecipe {
        owner: group,
        target: DependencyTarget::Unresolved {
            region: root,
            path: Box::new([]),
        },
        kind: DependencyKind::Reference,
        site: SourceSite {
            source,
            start: 1,
            end: 2,
        },
    });
    assert!(dependent.finish("profile").is_err());

    let (mut referenced, source, root, name, _container) = valid_shell();
    let missing_group = referenced.next_group_key().expect("missing group key");
    let ty = referenced
        .add_type_recipe(TypeRecipe {
            site: SourceSite {
                source,
                start: 2,
                end: 3,
            },
            node: TypeRecipeNode::Reference {
                path: Box::new([name]),
                resolution: TypeReferenceResolution::Group(missing_group),
                arguments: Box::new([]),
                dependency: DependencyLinkRecipe {
                    owner: missing_group,
                    kind: DependencyKind::Reference,
                },
            },
        })
        .expect("type");
    referenced
        .add_declaration(DeclarationRecipe {
            site: SourceSite {
                source,
                start: 1,
                end: 4,
            },
            binding_site: SourceSite {
                source,
                start: 1,
                end: 2,
            },
            region: root,
            lexical_scope: DeclarationLexicalScope::None,
            name,
            spaces: DeclarationSlots {
                value: true,
                r#type: false,
                namespace: false,
            },
            flags: DeclarationFlags::default(),
            payload: DeclarationPayload::Variable {
                annotation: Some(ty),
            },
        })
        .expect("declaration");
    assert!(referenced.finish("profile").is_err());
}

fn assert_validation_error(assembly: ProductAssembly, expected: ProductValidationError) {
    assert_eq!(
        assembly.finish("profile"),
        Err(ProductBuildError::Validation(expected))
    );
}

#[test]
fn relational_validation_reports_exact_owned_link_failures() {
    let (mut wrong_name, source, root, declaration_name, container) = valid_shell();
    let group_name = wrong_name.intern("Other").expect("group name");
    let declaration = wrong_name
        .add_declaration(DeclarationRecipe {
            site: SourceSite {
                source,
                start: 1,
                end: 4,
            },
            binding_site: SourceSite {
                source,
                start: 1,
                end: 2,
            },
            region: root,
            lexical_scope: DeclarationLexicalScope::None,
            name: declaration_name,
            spaces: DeclarationSlots {
                value: false,
                r#type: true,
                namespace: false,
            },
            flags: DeclarationFlags::default(),
            payload: DeclarationPayload::Interface {
                type_parameters: Box::new([]),
                members: Box::new([]),
                heritage: Box::new([]),
            },
        })
        .expect("declaration");
    wrong_name
        .add_group(DeclarationGroupRecipe {
            container,
            name: group_name,
            slots: DeclarationSlots {
                value: false,
                r#type: true,
                namespace: false,
            },
            fragments: Box::new([declaration]),
        })
        .expect("group");
    assert_validation_error(
        wrong_name,
        ProductValidationError::GroupFragmentName {
            group: 0,
            declaration: 0,
        },
    );

    let (mut wrong_region, source, root, name, container) = valid_shell();
    let child_container = wrong_region
        .add_container(ContainerRecipe {
            parent: Some(container),
            kind: ContainerKind::Namespace,
            name: Some(name),
            anchor: Some(SourceSite {
                source,
                start: 1,
                end: 2,
            }),
        })
        .expect("child container");
    let child = wrong_region.next_region_key().expect("child");
    wrong_region
        .add_region(RegionRecipe {
            parent: Some(root),
            container: child_container,
            kind: RegionKind::Namespace,
            site: SourceSite {
                source,
                start: 1,
                end: 9,
            },
        })
        .expect("child region");
    let declaration = wrong_region
        .add_declaration(DeclarationRecipe {
            site: SourceSite {
                source,
                start: 2,
                end: 5,
            },
            binding_site: SourceSite {
                source,
                start: 2,
                end: 3,
            },
            region: child,
            lexical_scope: DeclarationLexicalScope::None,
            name,
            spaces: DeclarationSlots {
                value: false,
                r#type: true,
                namespace: false,
            },
            flags: DeclarationFlags::default(),
            payload: DeclarationPayload::Interface {
                type_parameters: Box::new([]),
                members: Box::new([]),
                heritage: Box::new([]),
            },
        })
        .expect("declaration");
    wrong_region
        .add_group(DeclarationGroupRecipe {
            container,
            name,
            slots: DeclarationSlots {
                value: false,
                r#type: true,
                namespace: false,
            },
            fragments: Box::new([declaration]),
        })
        .expect("group");
    assert_validation_error(
        wrong_region,
        ProductValidationError::GroupFragmentContainer {
            group: 0,
            declaration: 0,
        },
    );

    let (mut missing, source, root, name, container) = valid_shell();
    let declaration = missing
        .add_declaration(DeclarationRecipe {
            site: SourceSite {
                source,
                start: 1,
                end: 4,
            },
            binding_site: SourceSite {
                source,
                start: 1,
                end: 2,
            },
            region: root,
            lexical_scope: DeclarationLexicalScope::None,
            name,
            spaces: DeclarationSlots {
                value: true,
                r#type: false,
                namespace: false,
            },
            flags: DeclarationFlags::default(),
            payload: DeclarationPayload::Variable { annotation: None },
        })
        .expect("declaration");
    missing
        .add_group(DeclarationGroupRecipe {
            container,
            name,
            slots: DeclarationSlots {
                value: true,
                r#type: false,
                namespace: false,
            },
            fragments: Box::new([declaration]),
        })
        .expect("group");
    assert_validation_error(
        missing,
        ProductValidationError::MissingAttachment {
            table: "values",
            group: 0,
            declaration: 0,
        },
    );

    let (mut extra, source, root, name, container) = valid_shell();
    let declaration = extra
        .add_declaration(DeclarationRecipe {
            site: SourceSite {
                source,
                start: 1,
                end: 4,
            },
            binding_site: SourceSite {
                source,
                start: 1,
                end: 2,
            },
            region: root,
            lexical_scope: DeclarationLexicalScope::None,
            name,
            spaces: DeclarationSlots {
                value: false,
                r#type: true,
                namespace: false,
            },
            flags: DeclarationFlags::default(),
            payload: DeclarationPayload::Interface {
                type_parameters: Box::new([]),
                members: Box::new([]),
                heritage: Box::new([]),
            },
        })
        .expect("declaration");
    let group = extra
        .add_group(DeclarationGroupRecipe {
            container,
            name,
            slots: DeclarationSlots {
                value: false,
                r#type: true,
                namespace: false,
            },
            fragments: Box::new([declaration]),
        })
        .expect("group");
    extra
        .add_value(ValueAttachmentRecipe { group, declaration })
        .expect("extra value attachment");
    assert_validation_error(
        extra,
        ProductValidationError::ExtraAttachment {
            table: "values",
            row: 0,
        },
    );

    let (mut bad_child, source, root, name, _container) = valid_shell();
    bad_child
        .add_declaration(DeclarationRecipe {
            site: SourceSite {
                source,
                start: 1,
                end: 4,
            },
            binding_site: SourceSite {
                source,
                start: 1,
                end: 2,
            },
            region: root,
            lexical_scope: DeclarationLexicalScope::None,
            name,
            spaces: DeclarationSlots {
                value: false,
                r#type: false,
                namespace: true,
            },
            flags: DeclarationFlags::default(),
            payload: DeclarationPayload::Namespace { child_region: root },
        })
        .expect("declaration");
    assert_validation_error(
        bad_child,
        ProductValidationError::InvalidChildRegion { declaration: 0 },
    );
}

#[test]
fn relational_validation_rejects_lexical_dependency_and_type_parameter_corruption() {
    let (mut lexical, source, root, name, _container) = valid_shell();
    let declaration = lexical
        .add_declaration(DeclarationRecipe {
            site: SourceSite {
                source,
                start: 1,
                end: 3,
            },
            binding_site: SourceSite {
                source,
                start: 1,
                end: 2,
            },
            region: root,
            lexical_scope: DeclarationLexicalScope::None,
            name,
            spaces: DeclarationSlots::default(),
            flags: DeclarationFlags::default(),
            payload: DeclarationPayload::Import,
        })
        .expect("declaration");
    lexical.add_lexical_step(LexicalStepRecipe {
        owner: declaration,
        kind: LexicalStepKind::Declaration,
        site: SourceSite {
            source,
            start: 4,
            end: 5,
        },
    });
    assert_validation_error(
        lexical,
        ProductValidationError::InvalidLexicalStep { row: 0 },
    );

    let (mut cross_source, source, root, name, container) = valid_shell();
    let other_source = cross_source.next_source_key().expect("other source");
    let other_root = cross_source.next_region_key().expect("other root");
    let other_name = cross_source.intern("y.d.ts").expect("other name");
    let digest = cross_source.intern("01").expect("other digest");
    cross_source
        .add_source(SourceRecipe {
            ordinal: 1,
            name: other_name,
            sha256: digest,
            byte_len: 10,
            classification: SourceClassification::Script,
            root_region: other_root,
        })
        .expect("other source row");
    cross_source
        .add_region(RegionRecipe {
            parent: None,
            container,
            kind: RegionKind::Source,
            site: SourceSite {
                source: other_source,
                start: 0,
                end: 10,
            },
        })
        .expect("other root row");
    let declaration = cross_source
        .add_declaration(DeclarationRecipe {
            site: SourceSite {
                source,
                start: 1,
                end: 3,
            },
            binding_site: SourceSite {
                source,
                start: 1,
                end: 2,
            },
            region: root,
            lexical_scope: DeclarationLexicalScope::None,
            name,
            spaces: DeclarationSlots::default(),
            flags: DeclarationFlags::default(),
            payload: DeclarationPayload::Import,
        })
        .expect("declaration");
    cross_source.add_lexical_step(LexicalStepRecipe {
        owner: declaration,
        kind: LexicalStepKind::Declaration,
        site: SourceSite {
            source: other_source,
            start: 1,
            end: 2,
        },
    });
    assert_validation_error(
        cross_source,
        ProductValidationError::InvalidLexicalStep { row: 0 },
    );

    let (mut dependent, source, root, name, container) = valid_shell();
    let owner_group = dependent.next_group_key().expect("owner group key");
    let ty = dependent
        .add_type_recipe(TypeRecipe {
            site: SourceSite {
                source,
                start: 2,
                end: 3,
            },
            node: TypeRecipeNode::Reference {
                path: Box::new([name]),
                resolution: TypeReferenceResolution::Group(owner_group),
                arguments: Box::new([]),
                dependency: DependencyLinkRecipe {
                    owner: owner_group,
                    kind: DependencyKind::Reference,
                },
            },
        })
        .expect("type");
    let declaration = dependent
        .add_declaration(DeclarationRecipe {
            site: SourceSite {
                source,
                start: 1,
                end: 4,
            },
            binding_site: SourceSite {
                source,
                start: 1,
                end: 2,
            },
            region: root,
            lexical_scope: DeclarationLexicalScope::None,
            name,
            spaces: DeclarationSlots {
                value: false,
                r#type: true,
                namespace: false,
            },
            flags: DeclarationFlags::default(),
            payload: DeclarationPayload::TypeAlias {
                type_parameters: Box::new([]),
                value: ty,
            },
        })
        .expect("declaration");
    dependent
        .add_group(DeclarationGroupRecipe {
            container,
            name,
            slots: DeclarationSlots {
                value: false,
                r#type: true,
                namespace: false,
            },
            fragments: Box::new([declaration]),
        })
        .expect("owner group");
    let wrong_target = dependent
        .add_group(DeclarationGroupRecipe {
            container,
            name,
            slots: DeclarationSlots::default(),
            fragments: Box::new([]),
        })
        .expect("wrong target");
    dependent.add_dependency(DependencyRecipe {
        owner: owner_group,
        target: DependencyTarget::Group(wrong_target),
        kind: DependencyKind::Reference,
        site: SourceSite {
            source,
            start: 2,
            end: 3,
        },
    });
    assert_validation_error(
        dependent,
        ProductValidationError::MissingDependency {
            table: "types",
            row: 0,
        },
    );

    let (mut scoped, source, root, name, _container) = valid_shell();
    let parameter = scoped
        .add_type_parameter(TypeParameterRecipe {
            site: SourceSite {
                source,
                start: 1,
                end: 2,
            },
            owner: SourceSite {
                source,
                start: 1,
                end: 9,
            },
            visibility: Box::new([SourceSite {
                source,
                start: 2,
                end: 3,
            }]),
            name,
            variance: Variance::Invariant,
            constant: false,
            constraint: None,
            default: None,
        })
        .expect("parameter");
    let owner_group = scoped.next_group_key().expect("owner group key");
    let ty = scoped
        .add_type_recipe(TypeRecipe {
            site: SourceSite {
                source,
                start: 4,
                end: 5,
            },
            node: TypeRecipeNode::Reference {
                path: Box::new([name]),
                resolution: TypeReferenceResolution::TypeParameter(parameter),
                arguments: Box::new([]),
                dependency: DependencyLinkRecipe {
                    owner: owner_group,
                    kind: DependencyKind::Reference,
                },
            },
        })
        .expect("type");
    scoped
        .add_declaration(DeclarationRecipe {
            site: SourceSite {
                source,
                start: 1,
                end: 9,
            },
            binding_site: SourceSite {
                source,
                start: 1,
                end: 2,
            },
            region: root,
            lexical_scope: DeclarationLexicalScope::None,
            name,
            spaces: DeclarationSlots {
                value: false,
                r#type: true,
                namespace: false,
            },
            flags: DeclarationFlags::default(),
            payload: DeclarationPayload::TypeAlias {
                type_parameters: Box::new([parameter]),
                value: ty,
            },
        })
        .expect("declaration");
    assert_validation_error(
        scoped,
        ProductValidationError::OutOfScopeTypeParameter {
            table: "types",
            row: 0,
        },
    );
}

fn dependency_control_shell() -> (
    ProductAssembly,
    typokat_library_product::SourceKey,
    typokat_library_product::GroupKey,
    typokat_library_product::GroupKey,
) {
    let (mut assembly, source, root, name, container) = valid_shell();
    let other_name = assembly.intern("Other").expect("other name");
    let owner = assembly.next_group_key().expect("owner key");
    let reference = assembly
        .add_type_recipe(TypeRecipe {
            site: SourceSite {
                source,
                start: 2,
                end: 3,
            },
            node: TypeRecipeNode::Reference {
                path: Box::new([name]),
                resolution: TypeReferenceResolution::Group(owner),
                arguments: Box::new([]),
                dependency: DependencyLinkRecipe {
                    owner,
                    kind: DependencyKind::Reference,
                },
            },
        })
        .expect("reference");
    let owner_declaration = assembly
        .add_declaration(DeclarationRecipe {
            site: SourceSite {
                source,
                start: 1,
                end: 4,
            },
            binding_site: SourceSite {
                source,
                start: 1,
                end: 2,
            },
            region: root,
            lexical_scope: DeclarationLexicalScope::None,
            name,
            spaces: DeclarationSlots {
                value: false,
                r#type: true,
                namespace: false,
            },
            flags: DeclarationFlags::default(),
            payload: DeclarationPayload::TypeAlias {
                type_parameters: Box::new([]),
                value: reference,
            },
        })
        .expect("owner declaration");
    let actual_owner = assembly
        .add_group(DeclarationGroupRecipe {
            container,
            name,
            slots: DeclarationSlots {
                value: false,
                r#type: true,
                namespace: false,
            },
            fragments: Box::new([owner_declaration]),
        })
        .expect("owner group");
    assert_eq!(actual_owner, owner);
    let other_declaration = assembly
        .add_declaration(DeclarationRecipe {
            site: SourceSite {
                source,
                start: 1,
                end: 4,
            },
            binding_site: SourceSite {
                source,
                start: 3,
                end: 4,
            },
            region: root,
            lexical_scope: DeclarationLexicalScope::None,
            name: other_name,
            spaces: DeclarationSlots {
                value: false,
                r#type: true,
                namespace: false,
            },
            flags: DeclarationFlags::default(),
            payload: DeclarationPayload::Interface {
                type_parameters: Box::new([]),
                members: Box::new([]),
                heritage: Box::new([]),
            },
        })
        .expect("other declaration");
    let other = assembly
        .add_group(DeclarationGroupRecipe {
            container,
            name: other_name,
            slots: DeclarationSlots {
                value: false,
                r#type: true,
                namespace: false,
            },
            fragments: Box::new([other_declaration]),
        })
        .expect("other group");
    (assembly, source, owner, other)
}

#[test]
fn dependency_multiset_reports_missing_extra_and_wrong_owner_exactly() {
    let (missing, _source, _owner, _other) = dependency_control_shell();
    assert_validation_error(
        missing,
        ProductValidationError::MissingDependency {
            table: "types",
            row: 0,
        },
    );

    let (mut extra, source, owner, _other) = dependency_control_shell();
    let edge = DependencyRecipe {
        owner,
        target: DependencyTarget::Group(owner),
        kind: DependencyKind::Reference,
        site: SourceSite {
            source,
            start: 2,
            end: 3,
        },
    };
    extra.add_dependency(edge.clone());
    extra.add_dependency(edge);
    assert_validation_error(extra, ProductValidationError::ExtraDependency { row: 1 });

    let (mut wrong_owner, source, owner, other) = dependency_control_shell();
    wrong_owner.add_dependency(DependencyRecipe {
        owner: other,
        target: DependencyTarget::Group(owner),
        kind: DependencyKind::Reference,
        site: SourceSite {
            source,
            start: 2,
            end: 3,
        },
    });
    assert_validation_error(
        wrong_owner,
        ProductValidationError::WrongDependencyOwner {
            table: "dependencies",
            row: 0,
        },
    );
}

#[test]
fn container_validation_rejects_bad_namespace_parent_and_external_ownership() {
    let (mut nested, source, root, name, global) = valid_shell();
    let outer_container = nested
        .add_container(ContainerRecipe {
            parent: Some(global),
            kind: ContainerKind::Namespace,
            name: Some(name),
            anchor: Some(SourceSite {
                source,
                start: 1,
                end: 2,
            }),
        })
        .expect("outer container");
    let wrong_child_container = nested
        .add_container(ContainerRecipe {
            parent: Some(global),
            kind: ContainerKind::Namespace,
            name: Some(name),
            anchor: Some(SourceSite {
                source,
                start: 3,
                end: 4,
            }),
        })
        .expect("wrong child container");
    let outer_region = nested.next_region_key().expect("outer region");
    nested
        .add_region(RegionRecipe {
            parent: Some(root),
            container: outer_container,
            kind: RegionKind::Namespace,
            site: SourceSite {
                source,
                start: 1,
                end: 9,
            },
        })
        .expect("outer region row");
    let child_region = nested.next_region_key().expect("child region");
    nested
        .add_region(RegionRecipe {
            parent: Some(outer_region),
            container: wrong_child_container,
            kind: RegionKind::Namespace,
            site: SourceSite {
                source,
                start: 3,
                end: 7,
            },
        })
        .expect("child region row");
    nested
        .add_declaration(DeclarationRecipe {
            site: SourceSite {
                source,
                start: 2,
                end: 8,
            },
            binding_site: SourceSite {
                source,
                start: 2,
                end: 3,
            },
            region: outer_region,
            lexical_scope: DeclarationLexicalScope::None,
            name,
            spaces: DeclarationSlots {
                value: false,
                r#type: false,
                namespace: true,
            },
            flags: DeclarationFlags::default(),
            payload: DeclarationPayload::Namespace { child_region },
        })
        .expect("namespace declaration");
    assert_validation_error(
        nested,
        ProductValidationError::InvalidChildRegion { declaration: 0 },
    );

    let mut duplicate = ProductAssembly::new();
    let source = duplicate.next_source_key().expect("source");
    let root = duplicate.next_region_key().expect("root");
    let external = duplicate
        .add_container(ContainerRecipe {
            parent: None,
            kind: ContainerKind::ExternalModule,
            name: None,
            anchor: Some(SourceSite {
                source,
                start: 0,
                end: 10,
            }),
        })
        .expect("external container");
    duplicate
        .add_container(ContainerRecipe {
            parent: None,
            kind: ContainerKind::ExternalModule,
            name: None,
            anchor: Some(SourceSite {
                source,
                start: 0,
                end: 10,
            }),
        })
        .expect("duplicate external container");
    let name = duplicate.intern("module.d.ts").expect("name");
    let digest = duplicate.intern("00").expect("digest");
    duplicate
        .add_source(SourceRecipe {
            ordinal: 0,
            name,
            sha256: digest,
            byte_len: 10,
            classification: SourceClassification::ExternalModule,
            root_region: root,
        })
        .expect("source row");
    duplicate
        .add_region(RegionRecipe {
            parent: None,
            container: external,
            kind: RegionKind::Source,
            site: SourceSite {
                source,
                start: 0,
                end: 10,
            },
        })
        .expect("root row");
    assert_validation_error(
        duplicate,
        ProductValidationError::InvalidContainer {
            table: "containers",
            row: 0,
        },
    );

    external_shell(10)
        .finish("profile")
        .expect("one exactly anchored external container");
    assert_validation_error(
        external_shell(9),
        ProductValidationError::InvalidContainer {
            table: "containers",
            row: 0,
        },
    );
}

#[test]
fn walker_source_has_no_wildcard_rest_patterns() {
    let source = include_str!("../src/walk.rs");
    assert!(!source.contains("{ .."));
    assert!(!source.contains(", .."));
}

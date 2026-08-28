use crate::schema::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductFieldClass {
    OwnedText,
    ProductKey,
    ProductKeyList,
    Scalar,
}

pub trait ProductFieldVisitor {
    fn visit_field(
        &mut self,
        class: ProductFieldClass,
        table: &'static str,
        row: usize,
        field: &'static str,
    );
}

impl LibraryDeclarationProduct {
    pub fn walk_fields<V: ProductFieldVisitor>(&self, visitor: &mut V) {
        let LibraryDeclarationProduct {
            profile: _profile,
            strings,
            sources,
            containers,
            regions,
            callable_scopes,
            declarations,
            groups,
            namespaces,
            values,
            intrinsic_markers,
            qualified_paths,
            qualified_path_segments,
            type_parameters,
            types,
            signatures,
            members,
            dependencies,
            lexical_steps,
        } = self;
        field(
            visitor,
            ProductFieldClass::ProductKey,
            "product",
            0,
            "profile",
        );
        for row in 0..strings.len() {
            field(
                visitor,
                ProductFieldClass::OwnedText,
                "strings",
                row,
                "value",
            );
        }
        for (row, source) in sources.iter().enumerate() {
            walk_source(visitor, row, source);
        }
        for (row, container) in containers.iter().enumerate() {
            walk_container(visitor, row, container);
        }
        for (row, region) in regions.iter().enumerate() {
            walk_region(visitor, row, region);
        }
        for (row, scope) in callable_scopes.iter().enumerate() {
            let CallableScopeRecipe { site, parent } = scope;
            walk_site(visitor, "callable_scopes", row, *site);
            match parent {
                LexicalScopeParent::Region(_region) => field(
                    visitor,
                    ProductFieldClass::ProductKey,
                    "callable_scopes",
                    row,
                    "parent.region",
                ),
                LexicalScopeParent::Callable(_callable) => field(
                    visitor,
                    ProductFieldClass::ProductKey,
                    "callable_scopes",
                    row,
                    "parent.callable",
                ),
            }
        }
        for (row, declaration) in declarations.iter().enumerate() {
            walk_declaration(visitor, row, declaration);
        }
        for (row, group) in groups.iter().enumerate() {
            walk_group(visitor, row, group);
        }
        for (row, attachment) in namespaces.iter().enumerate() {
            let NamespaceAttachmentRecipe {
                group: _group,
                declaration: _declaration,
            } = attachment;
            field(
                visitor,
                ProductFieldClass::ProductKey,
                "namespaces",
                row,
                "group",
            );
            field(
                visitor,
                ProductFieldClass::ProductKey,
                "namespaces",
                row,
                "declaration",
            );
        }
        for (row, attachment) in values.iter().enumerate() {
            let ValueAttachmentRecipe {
                group: _group,
                declaration: _declaration,
            } = attachment;
            field(
                visitor,
                ProductFieldClass::ProductKey,
                "values",
                row,
                "group",
            );
            field(
                visitor,
                ProductFieldClass::ProductKey,
                "values",
                row,
                "declaration",
            );
        }
        for (row, marker) in intrinsic_markers.iter().enumerate() {
            let IntrinsicMarkerRecipe {
                declaration: _declaration,
                marker,
            } = marker;
            field(
                visitor,
                ProductFieldClass::ProductKey,
                "intrinsic_markers",
                row,
                "declaration",
            );
            walk_intrinsic_marker(*marker);
            field(
                visitor,
                ProductFieldClass::Scalar,
                "intrinsic_markers",
                row,
                "marker",
            );
        }
        for (row, path) in qualified_paths.iter().enumerate() {
            let QualifiedPathRecipe {
                recipe: _recipe,
                role,
                resolution,
            } = path;
            field(
                visitor,
                ProductFieldClass::ProductKey,
                "qualified_paths",
                row,
                "recipe",
            );
            walk_qualified_path_role(*role);
            field(
                visitor,
                ProductFieldClass::Scalar,
                "qualified_paths",
                row,
                "role",
            );
            walk_qualified_path_resolution(*resolution);
            field(
                visitor,
                ProductFieldClass::Scalar,
                "qualified_paths",
                row,
                "resolution",
            );
        }
        for (row, segment) in qualified_path_segments.iter().enumerate() {
            let QualifiedPathSegmentRecipe {
                recipe: _recipe,
                ordinal: _ordinal,
                name: _name,
                site,
            } = segment;
            field(
                visitor,
                ProductFieldClass::ProductKey,
                "qualified_path_segments",
                row,
                "recipe",
            );
            field(
                visitor,
                ProductFieldClass::Scalar,
                "qualified_path_segments",
                row,
                "ordinal",
            );
            field(
                visitor,
                ProductFieldClass::ProductKey,
                "qualified_path_segments",
                row,
                "name",
            );
            walk_site(visitor, "qualified_path_segments", row, *site);
        }
        for (row, parameter) in type_parameters.iter().enumerate() {
            walk_type_parameter(visitor, row, parameter);
        }
        for (row, recipe) in types.iter().enumerate() {
            let TypeRecipe { site, node } = recipe;
            walk_site(visitor, "types", row, *site);
            walk_type_node(visitor, row, node);
        }
        for (row, signature) in signatures.iter().enumerate() {
            walk_signature(visitor, row, signature);
        }
        for (row, member) in members.iter().enumerate() {
            walk_member(visitor, row, member);
        }
        for (row, dependency) in dependencies.iter().enumerate() {
            walk_dependency(visitor, row, dependency);
        }
        for (row, step) in lexical_steps.iter().enumerate() {
            let LexicalStepRecipe {
                owner: _owner,
                kind,
                site,
            } = step;
            field(
                visitor,
                ProductFieldClass::ProductKey,
                "lexical_steps",
                row,
                "owner",
            );
            walk_lexical_kind(*kind);
            field(
                visitor,
                ProductFieldClass::Scalar,
                "lexical_steps",
                row,
                "kind",
            );
            walk_site(visitor, "lexical_steps", row, *site);
        }
    }
}

fn walk_container_kind(kind: ContainerKind) {
    match kind {
        ContainerKind::CompilationGlobal
        | ContainerKind::ExternalModule
        | ContainerKind::Namespace
        | ContainerKind::AmbientExternalModule => {}
    }
}

fn walk_container<V: ProductFieldVisitor>(
    visitor: &mut V,
    row: usize,
    container: &ContainerRecipe,
) {
    let ContainerRecipe {
        parent: _parent,
        kind,
        name: _name,
        anchor,
    } = container;
    walk_container_kind(*kind);
    emit(
        visitor,
        "containers",
        row,
        &[
            (ProductFieldClass::ProductKey, "parent"),
            (ProductFieldClass::Scalar, "kind"),
            (ProductFieldClass::ProductKey, "name"),
        ],
    );
    if let Some(site) = anchor {
        walk_site(visitor, "containers", row, *site);
    }
}

fn walk_source<V: ProductFieldVisitor>(visitor: &mut V, row: usize, source: &SourceRecipe) {
    let SourceRecipe {
        ordinal: _ordinal,
        name: _name,
        sha256: _sha256,
        byte_len: _byte_len,
        classification,
        root_region: _root_region,
    } = source;
    walk_source_classification(*classification);
    emit(
        visitor,
        "sources",
        row,
        &[
            (ProductFieldClass::Scalar, "ordinal"),
            (ProductFieldClass::ProductKey, "name"),
            (ProductFieldClass::ProductKey, "sha256"),
            (ProductFieldClass::Scalar, "byte_len"),
            (ProductFieldClass::Scalar, "classification"),
            (ProductFieldClass::ProductKey, "root_region"),
        ],
    );
}

fn walk_region<V: ProductFieldVisitor>(visitor: &mut V, row: usize, region: &RegionRecipe) {
    let RegionRecipe {
        parent: _parent,
        container: _container,
        kind,
        site,
    } = region;
    walk_region_kind(*kind);
    emit(
        visitor,
        "regions",
        row,
        &[
            (ProductFieldClass::ProductKey, "parent"),
            (ProductFieldClass::ProductKey, "container"),
            (ProductFieldClass::Scalar, "kind"),
        ],
    );
    walk_site(visitor, "regions", row, *site);
}

fn walk_declaration<V: ProductFieldVisitor>(
    visitor: &mut V,
    row: usize,
    declaration: &DeclarationRecipe,
) {
    let DeclarationRecipe {
        site,
        binding_site,
        region: _region,
        lexical_scope,
        name: _name,
        spaces,
        flags,
        payload,
    } = declaration;
    walk_site(visitor, "declarations", row, *site);
    walk_site(visitor, "declarations", row, *binding_site);
    emit(
        visitor,
        "declarations",
        row,
        &[
            (ProductFieldClass::ProductKey, "region"),
            (ProductFieldClass::ProductKey, "name"),
        ],
    );
    match lexical_scope {
        DeclarationLexicalScope::None => field(
            visitor,
            ProductFieldClass::Scalar,
            "declarations",
            row,
            "lexical_scope.none",
        ),
        DeclarationLexicalScope::Region(_region) => field(
            visitor,
            ProductFieldClass::ProductKey,
            "declarations",
            row,
            "lexical_scope.region",
        ),
        DeclarationLexicalScope::Callable(_callable) => field(
            visitor,
            ProductFieldClass::ProductKey,
            "declarations",
            row,
            "lexical_scope.callable",
        ),
    }
    walk_declaration_slots(visitor, "declarations", row, spaces);
    walk_declaration_flags(visitor, row, flags);
    walk_declaration_payload(visitor, row, payload);
}

fn walk_declaration_flags<V: ProductFieldVisitor>(
    visitor: &mut V,
    row: usize,
    flags: &DeclarationFlags,
) {
    let DeclarationFlags {
        ambient: _ambient,
        exported: _exported,
        default_export: _default_export,
        constant: _constant,
        readonly: _readonly,
        r#abstract: _abstract,
    } = flags;
    for name in [
        "flags.ambient",
        "flags.exported",
        "flags.default_export",
        "flags.constant",
        "flags.readonly",
        "flags.abstract",
    ] {
        field(
            visitor,
            ProductFieldClass::Scalar,
            "declarations",
            row,
            name,
        );
    }
}

fn walk_declaration_slots<V: ProductFieldVisitor>(
    visitor: &mut V,
    table: &'static str,
    row: usize,
    slots: &DeclarationSlots,
) {
    let DeclarationSlots {
        value: _value,
        r#type: _type,
        namespace: _namespace,
    } = slots;
    emit(
        visitor,
        table,
        row,
        &[
            (ProductFieldClass::Scalar, "spaces.value"),
            (ProductFieldClass::Scalar, "spaces.type"),
            (ProductFieldClass::Scalar, "spaces.namespace"),
        ],
    );
}

fn walk_declaration_payload<V: ProductFieldVisitor>(
    visitor: &mut V,
    row: usize,
    payload: &DeclarationPayload,
) {
    match payload {
        DeclarationPayload::Variable {
            annotation: _annotation,
        }
        | DeclarationPayload::Parameter {
            annotation: _annotation,
        }
        | DeclarationPayload::CatchParameter {
            annotation: _annotation,
        } => field(
            visitor,
            ProductFieldClass::ProductKey,
            "declarations",
            row,
            "payload.annotation",
        ),
        DeclarationPayload::Function {
            signature: _signature,
        } => field(
            visitor,
            ProductFieldClass::ProductKey,
            "declarations",
            row,
            "payload.signature",
        ),
        DeclarationPayload::Class {
            type_parameters: _type_parameters,
            members: _members,
            heritage: _heritage,
        }
        | DeclarationPayload::Interface {
            type_parameters: _type_parameters,
            members: _members,
            heritage: _heritage,
        } => emit(
            visitor,
            "declarations",
            row,
            &[
                (ProductFieldClass::ProductKeyList, "payload.type_parameters"),
                (ProductFieldClass::ProductKeyList, "payload.members"),
                (ProductFieldClass::ProductKeyList, "payload.heritage"),
            ],
        ),
        DeclarationPayload::TypeAlias {
            type_parameters: _type_parameters,
            value: _value,
        } => emit(
            visitor,
            "declarations",
            row,
            &[
                (ProductFieldClass::ProductKeyList, "payload.type_parameters"),
                (ProductFieldClass::ProductKey, "payload.value"),
            ],
        ),
        DeclarationPayload::Enum { members: _members } => field(
            visitor,
            ProductFieldClass::ProductKeyList,
            "declarations",
            row,
            "payload.members",
        ),
        DeclarationPayload::Namespace {
            child_region: _child_region,
        }
        | DeclarationPayload::Global {
            child_region: _child_region,
        } => field(
            visitor,
            ProductFieldClass::ProductKey,
            "declarations",
            row,
            "payload.child_region",
        ),
        DeclarationPayload::Import
        | DeclarationPayload::ImportEquals
        | DeclarationPayload::NamespaceExport
        | DeclarationPayload::DefaultExportExpression => field(
            visitor,
            ProductFieldClass::Scalar,
            "declarations",
            row,
            "payload.kind",
        ),
    }
}

fn walk_group<V: ProductFieldVisitor>(visitor: &mut V, row: usize, group: &DeclarationGroupRecipe) {
    let DeclarationGroupRecipe {
        container: _container,
        name: _name,
        slots,
        fragments: _fragments,
    } = group;
    emit(
        visitor,
        "groups",
        row,
        &[
            (ProductFieldClass::ProductKey, "container"),
            (ProductFieldClass::ProductKey, "name"),
        ],
    );
    walk_declaration_slots(visitor, "groups", row, slots);
    field(
        visitor,
        ProductFieldClass::ProductKeyList,
        "groups",
        row,
        "fragments",
    );
}

fn walk_type_parameter<V: ProductFieldVisitor>(
    visitor: &mut V,
    row: usize,
    parameter: &TypeParameterRecipe,
) {
    let TypeParameterRecipe {
        site,
        owner,
        visibility,
        name: _name,
        variance,
        constant: _constant,
        constraint: _constraint,
        default: _default,
    } = parameter;
    walk_site(visitor, "type_parameters", row, *site);
    walk_site(visitor, "type_parameters", row, *owner);
    for visibility_site in visibility {
        walk_site(visitor, "type_parameters", row, *visibility_site);
    }
    walk_variance(*variance);
    emit(
        visitor,
        "type_parameters",
        row,
        &[
            (ProductFieldClass::ProductKeyList, "visibility"),
            (ProductFieldClass::ProductKey, "name"),
            (ProductFieldClass::Scalar, "variance"),
            (ProductFieldClass::Scalar, "constant"),
            (ProductFieldClass::ProductKey, "constraint"),
            (ProductFieldClass::ProductKey, "default"),
        ],
    );
}

fn walk_type_node<V: ProductFieldVisitor>(visitor: &mut V, row: usize, node: &TypeRecipeNode) {
    match node {
        TypeRecipeNode::Keyword(keyword) => {
            walk_keyword(*keyword);
            field(
                visitor,
                ProductFieldClass::Scalar,
                "types",
                row,
                "node.keyword",
            );
        }
        TypeRecipeNode::Literal(literal) => walk_literal(visitor, row, *literal),
        TypeRecipeNode::Reference {
            path: _path,
            resolution,
            arguments: _arguments,
            dependency,
        } => {
            emit(
                visitor,
                "types",
                row,
                &[
                    (ProductFieldClass::ProductKeyList, "node.path"),
                    (ProductFieldClass::ProductKeyList, "node.arguments"),
                ],
            );
            walk_type_resolution(visitor, "types", row, resolution);
            walk_dependency_link(visitor, "types", row, dependency);
        }
        TypeRecipeNode::Query {
            path: _path,
            resolution,
            dependency,
        } => {
            field(
                visitor,
                ProductFieldClass::ProductKeyList,
                "types",
                row,
                "node.path",
            );
            walk_value_resolution(visitor, "types", row, resolution);
            walk_dependency_link(visitor, "types", row, dependency);
        }
        TypeRecipeNode::Import {
            argument: _argument,
            qualifier: _qualifier,
            arguments: _arguments,
        } => emit(
            visitor,
            "types",
            row,
            &[
                (ProductFieldClass::ProductKey, "node.argument"),
                (ProductFieldClass::ProductKeyList, "node.qualifier"),
                (ProductFieldClass::ProductKeyList, "node.arguments"),
            ],
        ),
        TypeRecipeNode::Array(_element) => field(
            visitor,
            ProductFieldClass::ProductKey,
            "types",
            row,
            "node.element",
        ),
        TypeRecipeNode::Tuple { elements } => {
            for element in elements {
                let TupleElementRecipe {
                    site,
                    name: _name,
                    value: _value,
                    optional: _optional,
                    rest: _rest,
                } = element;
                emit(
                    visitor,
                    "types",
                    row,
                    &[
                        (ProductFieldClass::ProductKey, "node.element.name"),
                        (ProductFieldClass::ProductKey, "node.element.value"),
                        (ProductFieldClass::Scalar, "node.element.optional"),
                        (ProductFieldClass::Scalar, "node.element.rest"),
                    ],
                );
                let SourceSite {
                    source: _source,
                    start: _start,
                    end: _end,
                } = site;
                emit(
                    visitor,
                    "types",
                    row,
                    &[
                        (ProductFieldClass::ProductKey, "node.element.site.source"),
                        (ProductFieldClass::Scalar, "node.element.site.start"),
                        (ProductFieldClass::Scalar, "node.element.site.end"),
                    ],
                );
            }
        }
        TypeRecipeNode::Union(_types) | TypeRecipeNode::Intersection(_types) => field(
            visitor,
            ProductFieldClass::ProductKeyList,
            "types",
            row,
            "node.types",
        ),
        TypeRecipeNode::Function(_signature) => field(
            visitor,
            ProductFieldClass::ProductKey,
            "types",
            row,
            "node.signature",
        ),
        TypeRecipeNode::Constructor {
            signature: _signature,
            r#abstract: _abstract,
        } => emit(
            visitor,
            "types",
            row,
            &[
                (ProductFieldClass::ProductKey, "node.signature"),
                (ProductFieldClass::Scalar, "node.abstract"),
            ],
        ),
        TypeRecipeNode::Object(_members) => field(
            visitor,
            ProductFieldClass::ProductKeyList,
            "types",
            row,
            "node.members",
        ),
        TypeRecipeNode::Operator {
            operator,
            operand: _operand,
        } => {
            walk_type_operator(*operator);
            emit(
                visitor,
                "types",
                row,
                &[
                    (ProductFieldClass::Scalar, "node.operator"),
                    (ProductFieldClass::ProductKey, "node.operand"),
                ],
            );
        }
        TypeRecipeNode::IndexedAccess {
            object: _object,
            index: _index,
        } => emit(
            visitor,
            "types",
            row,
            &[
                (ProductFieldClass::ProductKey, "node.object"),
                (ProductFieldClass::ProductKey, "node.index"),
            ],
        ),
        TypeRecipeNode::Mapped {
            parameter: _parameter,
            name: _name,
            value: _value,
            optional,
            readonly,
        } => {
            walk_mapped_modifier(*optional);
            walk_mapped_modifier(*readonly);
            emit(
                visitor,
                "types",
                row,
                &[
                    (ProductFieldClass::ProductKey, "node.parameter"),
                    (ProductFieldClass::ProductKey, "node.name"),
                    (ProductFieldClass::ProductKey, "node.value"),
                    (ProductFieldClass::Scalar, "node.optional"),
                    (ProductFieldClass::Scalar, "node.readonly"),
                ],
            );
        }
        TypeRecipeNode::Conditional {
            check: _check,
            extends: _extends,
            when_true: _when_true,
            when_false: _when_false,
        } => emit(
            visitor,
            "types",
            row,
            &[
                (ProductFieldClass::ProductKey, "node.check"),
                (ProductFieldClass::ProductKey, "node.extends"),
                (ProductFieldClass::ProductKey, "node.when_true"),
                (ProductFieldClass::ProductKey, "node.when_false"),
            ],
        ),
        TypeRecipeNode::Infer(_parameter) => field(
            visitor,
            ProductFieldClass::ProductKey,
            "types",
            row,
            "node.parameter",
        ),
        TypeRecipeNode::Template {
            texts: _texts,
            holes: _holes,
        } => emit(
            visitor,
            "types",
            row,
            &[
                (ProductFieldClass::ProductKeyList, "node.texts"),
                (ProductFieldClass::ProductKeyList, "node.holes"),
            ],
        ),
        TypeRecipeNode::Predicate {
            parameter: _parameter,
            asserts: _asserts,
            value: _value,
        } => emit(
            visitor,
            "types",
            row,
            &[
                (ProductFieldClass::ProductKey, "node.parameter"),
                (ProductFieldClass::Scalar, "node.asserts"),
                (ProductFieldClass::ProductKey, "node.value"),
            ],
        ),
    }
}

fn walk_literal<V: ProductFieldVisitor>(visitor: &mut V, row: usize, literal: LiteralRecipe) {
    match literal {
        LiteralRecipe::String(_text) | LiteralRecipe::BigInt(_text) => field(
            visitor,
            ProductFieldClass::ProductKey,
            "types",
            row,
            "node.literal.text",
        ),
        LiteralRecipe::NumberBits(_number) => field(
            visitor,
            ProductFieldClass::Scalar,
            "types",
            row,
            "node.literal.value",
        ),
        LiteralRecipe::Boolean(_boolean) => field(
            visitor,
            ProductFieldClass::Scalar,
            "types",
            row,
            "node.literal.value",
        ),
    }
}

fn walk_type_resolution<V: ProductFieldVisitor>(
    visitor: &mut V,
    table: &'static str,
    row: usize,
    resolution: &TypeReferenceResolution,
) {
    let field_name = match resolution {
        TypeReferenceResolution::Group(_group) => "resolution.group",
        TypeReferenceResolution::TypeParameter(_parameter) => "resolution.type_parameter",
        TypeReferenceResolution::Unresolved { region: _region } => "resolution.region",
    };
    field(
        visitor,
        ProductFieldClass::ProductKey,
        table,
        row,
        field_name,
    );
}

fn walk_value_resolution<V: ProductFieldVisitor>(
    visitor: &mut V,
    table: &'static str,
    row: usize,
    resolution: &ValueReferenceResolution,
) {
    let field_name = match resolution {
        ValueReferenceResolution::Group(_group) => "resolution.group",
        ValueReferenceResolution::IntrinsicGlobalValue => "resolution.intrinsic_global_value",
        ValueReferenceResolution::Unresolved { region: _region } => "resolution.region",
    };
    field(
        visitor,
        ProductFieldClass::ProductKey,
        table,
        row,
        field_name,
    );
}

fn walk_signature<V: ProductFieldVisitor>(
    visitor: &mut V,
    row: usize,
    signature: &SignatureRecipe,
) {
    let SignatureRecipe {
        type_parameters: _type_parameters,
        parameters,
        result: _result,
    } = signature;
    field(
        visitor,
        ProductFieldClass::ProductKeyList,
        "signatures",
        row,
        "type_parameters",
    );
    for parameter in parameters {
        walk_parameter(visitor, row, parameter);
    }
    field(
        visitor,
        ProductFieldClass::ProductKey,
        "signatures",
        row,
        "result",
    );
}

fn walk_parameter<V: ProductFieldVisitor>(
    visitor: &mut V,
    row: usize,
    parameter: &ParameterRecipe,
) {
    let ParameterRecipe {
        pattern,
        value: _value,
        optional: _optional,
        rest: _rest,
        this_parameter: _this_parameter,
    } = parameter;
    match pattern {
        ParameterPattern::Identifier(_identifier) => field(
            visitor,
            ProductFieldClass::ProductKey,
            "signatures",
            row,
            "parameter.identifier",
        ),
        ParameterPattern::Array(_names) => field(
            visitor,
            ProductFieldClass::ProductKeyList,
            "signatures",
            row,
            "parameter.array",
        ),
        ParameterPattern::Object(_names) => field(
            visitor,
            ProductFieldClass::ProductKeyList,
            "signatures",
            row,
            "parameter.object",
        ),
    }
    emit(
        visitor,
        "signatures",
        row,
        &[
            (ProductFieldClass::ProductKey, "parameter.value"),
            (ProductFieldClass::Scalar, "parameter.optional"),
            (ProductFieldClass::Scalar, "parameter.rest"),
            (ProductFieldClass::Scalar, "parameter.this"),
        ],
    );
}

fn walk_member<V: ProductFieldVisitor>(visitor: &mut V, row: usize, member: &MemberRecipe) {
    let MemberRecipe {
        site,
        kind,
        name,
        optional: _optional,
        readonly: _readonly,
        static_member: _static_member,
        accessibility,
        r#abstract: _abstract,
        value: _value,
        signature: _signature,
    } = member;
    walk_site(visitor, "members", row, *site);
    walk_member_kind(*kind);
    if let Some(accessibility) = accessibility {
        walk_accessibility(*accessibility);
    }
    field(visitor, ProductFieldClass::Scalar, "members", row, "kind");
    walk_property_name(visitor, row, name.as_ref());
    emit(
        visitor,
        "members",
        row,
        &[
            (ProductFieldClass::Scalar, "optional"),
            (ProductFieldClass::Scalar, "readonly"),
            (ProductFieldClass::Scalar, "static_member"),
            (ProductFieldClass::Scalar, "accessibility"),
            (ProductFieldClass::Scalar, "abstract"),
            (ProductFieldClass::ProductKey, "value"),
            (ProductFieldClass::ProductKey, "signature"),
        ],
    );
}

fn walk_property_name<V: ProductFieldVisitor>(
    visitor: &mut V,
    row: usize,
    name: Option<&PropertyNameRecipe>,
) {
    match name {
        Some(PropertyNameRecipe::Identifier(_text))
        | Some(PropertyNameRecipe::String(_text))
        | Some(PropertyNameRecipe::Private(_text)) => field(
            visitor,
            ProductFieldClass::ProductKey,
            "members",
            row,
            "name.text",
        ),
        Some(PropertyNameRecipe::NumberBits(_number)) => field(
            visitor,
            ProductFieldClass::Scalar,
            "members",
            row,
            "name.number",
        ),
        Some(PropertyNameRecipe::Computed { site, value }) => {
            walk_site(visitor, "members", row, *site);
            match value {
                SymbolicValueNameRecipe::Path {
                    path: _path,
                    resolution,
                    dependency,
                } => {
                    field(
                        visitor,
                        ProductFieldClass::ProductKeyList,
                        "members",
                        row,
                        "name.computed.path",
                    );
                    walk_value_resolution(visitor, "members", row, resolution);
                    walk_dependency_link(visitor, "members", row, dependency);
                }
                SymbolicValueNameRecipe::String(_text) => field(
                    visitor,
                    ProductFieldClass::ProductKey,
                    "members",
                    row,
                    "name.computed.string",
                ),
                SymbolicValueNameRecipe::NumberBits(_number) => field(
                    visitor,
                    ProductFieldClass::Scalar,
                    "members",
                    row,
                    "name.computed.number",
                ),
            }
        }
        None => field(
            visitor,
            ProductFieldClass::Scalar,
            "members",
            row,
            "name.none",
        ),
    }
}

fn walk_dependency_link<V: ProductFieldVisitor>(
    visitor: &mut V,
    table: &'static str,
    row: usize,
    link: &DependencyLinkRecipe,
) {
    let DependencyLinkRecipe {
        owner: _owner,
        kind,
    } = link;
    field(
        visitor,
        ProductFieldClass::ProductKey,
        table,
        row,
        "dependency.owner",
    );
    walk_dependency_kind(*kind);
    field(
        visitor,
        ProductFieldClass::Scalar,
        table,
        row,
        "dependency.kind",
    );
}

fn walk_dependency<V: ProductFieldVisitor>(
    visitor: &mut V,
    row: usize,
    dependency: &DependencyRecipe,
) {
    let DependencyRecipe {
        owner: _owner,
        target,
        kind,
        site,
    } = dependency;
    field(
        visitor,
        ProductFieldClass::ProductKey,
        "dependencies",
        row,
        "owner",
    );
    match target {
        DependencyTarget::Group(_group) => field(
            visitor,
            ProductFieldClass::ProductKey,
            "dependencies",
            row,
            "target.group",
        ),
        DependencyTarget::TypeParameter(_parameter) => field(
            visitor,
            ProductFieldClass::ProductKey,
            "dependencies",
            row,
            "target.type_parameter",
        ),
        DependencyTarget::IntrinsicGlobalValue => field(
            visitor,
            ProductFieldClass::Scalar,
            "dependencies",
            row,
            "target.intrinsic_global_value",
        ),
        DependencyTarget::Unresolved {
            region: _region,
            path: _path,
        } => emit(
            visitor,
            "dependencies",
            row,
            &[
                (ProductFieldClass::ProductKey, "target.region"),
                (ProductFieldClass::ProductKeyList, "target.path"),
            ],
        ),
    }
    walk_dependency_kind(*kind);
    field(
        visitor,
        ProductFieldClass::Scalar,
        "dependencies",
        row,
        "kind",
    );
    walk_site(visitor, "dependencies", row, *site);
}

fn walk_site<V: ProductFieldVisitor>(
    visitor: &mut V,
    table: &'static str,
    row: usize,
    site: SourceSite,
) {
    let SourceSite {
        source: _source,
        start: _start,
        end: _end,
    } = site;
    emit(
        visitor,
        table,
        row,
        &[
            (ProductFieldClass::ProductKey, "site.source"),
            (ProductFieldClass::Scalar, "site.start"),
            (ProductFieldClass::Scalar, "site.end"),
        ],
    );
}

fn walk_source_classification(value: SourceClassification) {
    match value {
        SourceClassification::Script | SourceClassification::ExternalModule => {}
    }
}

fn walk_region_kind(value: RegionKind) {
    match value {
        RegionKind::Source
        | RegionKind::Namespace
        | RegionKind::GlobalAugmentation
        | RegionKind::AmbientExternalModule => {}
    }
}

fn walk_variance(value: Variance) {
    match value {
        Variance::Invariant | Variance::In | Variance::Out | Variance::InOut => {}
    }
}

fn walk_keyword(value: KeywordKind) {
    match value {
        KeywordKind::Any
        | KeywordKind::Unknown
        | KeywordKind::Never
        | KeywordKind::Void
        | KeywordKind::Null
        | KeywordKind::Undefined
        | KeywordKind::Boolean
        | KeywordKind::Number
        | KeywordKind::BigInt
        | KeywordKind::String
        | KeywordKind::Symbol
        | KeywordKind::Object
        | KeywordKind::Intrinsic
        | KeywordKind::This => {}
    }
}

fn walk_intrinsic_marker(value: IntrinsicMarker) {
    match value {
        IntrinsicMarker::Uppercase
        | IntrinsicMarker::Lowercase
        | IntrinsicMarker::Capitalize
        | IntrinsicMarker::Uncapitalize
        | IntrinsicMarker::NoInfer
        | IntrinsicMarker::BuiltinIteratorReturn => {}
    }
}

fn walk_type_operator(value: TypeOperator) {
    match value {
        TypeOperator::Keyof | TypeOperator::Readonly | TypeOperator::Unique => {}
    }
}

fn walk_mapped_modifier(value: MappedModifier) {
    match value {
        MappedModifier::Preserve | MappedModifier::Add | MappedModifier::Remove => {}
    }
}

fn walk_member_kind(value: MemberKind) {
    match value {
        MemberKind::Property
        | MemberKind::Method
        | MemberKind::Getter
        | MemberKind::Setter
        | MemberKind::Call
        | MemberKind::Construct
        | MemberKind::Index
        | MemberKind::Constructor => {}
    }
}

fn walk_accessibility(value: Accessibility) {
    match value {
        Accessibility::Public | Accessibility::Protected | Accessibility::Private => {}
    }
}

fn walk_dependency_kind(value: DependencyKind) {
    match value {
        DependencyKind::Reference
        | DependencyKind::Heritage
        | DependencyKind::Query
        | DependencyKind::Import
        | DependencyKind::IndexedAccess
        | DependencyKind::Constraint => {}
    }
}

fn walk_lexical_kind(value: LexicalStepKind) {
    match value {
        LexicalStepKind::Declaration | LexicalStepKind::Heritage | LexicalStepKind::Member => {}
    }
}

fn emit<V: ProductFieldVisitor>(
    visitor: &mut V,
    table: &'static str,
    row: usize,
    fields: &[(ProductFieldClass, &'static str)],
) {
    for (class, name) in fields {
        field(visitor, *class, table, row, name);
    }
}

fn walk_qualified_path_role(role: QualifiedPathRole) {
    match role {
        QualifiedPathRole::Reference
        | QualifiedPathRole::Query
        | QualifiedPathRole::ImportQualifier => {}
    }
}

fn walk_qualified_path_resolution(resolution: QualifiedPathResolutionOutcome) {
    match resolution {
        QualifiedPathResolutionOutcome::Resolved => {}
        QualifiedPathResolutionOutcome::Unresolved {
            cause,
            failing_segment_ordinal: _,
        } => match cause {
            QualifiedPathUnresolvedCause::MissingRoot
            | QualifiedPathUnresolvedCause::TypeOnlyRoot
            | QualifiedPathUnresolvedCause::MissingMember
            | QualifiedPathUnresolvedCause::TypeOnlyIntermediate
            | QualifiedPathUnresolvedCause::ValueOnlyLeaf
            | QualifiedPathUnresolvedCause::NamespaceOnlyLeaf
            | QualifiedPathUnresolvedCause::Unavailable => {}
            QualifiedPathUnresolvedCause::Deferred(reason) => match reason {
                QualifiedPathDeferredReason::Import | QualifiedPathDeferredReason::Enum => {}
            },
        },
    }
}

fn field<V: ProductFieldVisitor>(
    visitor: &mut V,
    class: ProductFieldClass,
    table: &'static str,
    row: usize,
    name: &'static str,
) {
    visitor.visit_field(class, table, row, name);
}

use super::*;
use crate::types::intern::{DerivationEdge, DerivationId, DerivedType};
use crate::types::repr::DeclaredRecipeNode;

pub(super) fn derive_substitution(
    interner: &mut Interner,
    template: TypeId,
    result: TypeId,
    mapper: &FxHashMap<TypeParamId, DerivedType>,
) -> DerivedType {
    derive_node(
        interner,
        template,
        result,
        mapper,
        &mut FxHashMap::default(),
    )
}

fn derive_node(
    interner: &mut Interner,
    template: TypeId,
    result: TypeId,
    mapper: &FxHashMap<TypeParamId, DerivedType>,
    derived: &mut FxHashMap<(TypeId, TypeId), DerivationId>,
) -> DerivedType {
    if let Some(mapped) = interner
        .store()
        .type_param(template)
        .and_then(|parameter| mapper.get(&parameter.id))
        .copied()
    {
        return if mapped.ty == result {
            mapped
        } else {
            DerivedType::plain(result)
        };
    }
    if let Some(&derivation) = derived.get(&(template, result)) {
        return DerivedType {
            ty: result,
            derivation: Some(derivation),
        };
    }

    let pairs = if interner.store().tag(template) == TypeTag::Declared
        && interner.store().tag(result) == TypeTag::Declared
    {
        declared_children(interner, template, result)
    } else {
        structural_children(interner.store(), template, result, mapper)
    };
    let structural = matches!(
        interner.store().tag(result),
        TypeTag::Object | TypeTag::Function | TypeTag::Declared
    );
    if template == result && pairs.is_empty() || (!structural && pairs.is_empty()) {
        return DerivedType::plain(result);
    }

    let identity = declared_application_template(interner.store(), template).unwrap_or(template);
    let derivation = interner.intern_derivation(result, identity, Vec::new());
    derived.insert((template, result), derivation);
    let mut children = Vec::new();
    for (edge, template_child, result_child) in pairs {
        let child = derive_node(interner, template_child, result_child, mapper, derived);
        if let Some(derivation) = child.derivation {
            children.push((edge, derivation));
        }
    }
    let completed = interner.complete_derivation(derivation, result, identity, children);
    debug_assert!(completed, "fresh derivation reservations complete locally");
    DerivedType {
        ty: result,
        derivation: Some(derivation),
    }
}

fn declared_application_template(store: &Store, ty: TypeId) -> Option<TypeId> {
    let declared = store.declared_type(ty)?;
    let recipe = store.declared_recipe(declared.recipe)?;
    match &recipe.node {
        DeclaredRecipeNode::Application { template, .. } => Some(*template),
        _ => None,
    }
}

fn declared_children(
    interner: &mut Interner,
    template: TypeId,
    result: TypeId,
) -> Vec<(DerivationEdge, TypeId, TypeId)> {
    let (Some(template_declared), Some(result_declared)) = (
        interner.store().declared_type(template).cloned(),
        interner.store().declared_type(result).cloned(),
    ) else {
        return Vec::new();
    };
    if template_declared.recipe != result_declared.recipe
        || template_declared.mapper.len() != result_declared.mapper.len()
    {
        return Vec::new();
    }
    let mut children = Vec::new();
    for (index, ((template_parameter, template_value), (result_parameter, result_value))) in
        template_declared
            .mapper
            .iter()
            .zip(&result_declared.mapper)
            .enumerate()
    {
        if template_parameter != result_parameter {
            return Vec::new();
        }
        children.push((
            DerivationEdge::DeclaredMapper(index),
            *template_value,
            *result_value,
        ));
    }
    let arguments = interner
        .store()
        .declared_recipe(template_declared.recipe)
        .and_then(|recipe| match &recipe.node {
            DeclaredRecipeNode::Application { arguments, .. } => Some(arguments.clone()),
            _ => None,
        })
        .unwrap_or_default();
    for (index, argument) in arguments.into_iter().enumerate() {
        let template_child =
            interner.intern_declared(argument, template_declared.mapper.iter().copied());
        let result_child =
            interner.intern_declared(argument, result_declared.mapper.iter().copied());
        children.push((
            DerivationEdge::DeclaredArgument(index),
            template_child,
            result_child,
        ));
    }
    children
}

fn structural_children(
    store: &Store,
    template: TypeId,
    result: TypeId,
    mapper: &FxHashMap<TypeParamId, DerivedType>,
) -> Vec<(DerivationEdge, TypeId, TypeId)> {
    if store.tag(template) != store.tag(result) {
        return Vec::new();
    }
    match store.tag(template) {
        TypeTag::Object => {
            let (Some(template), Some(result)) =
                (store.object_type(template), store.object_type(result))
            else {
                return Vec::new();
            };
            let mut children = Vec::new();
            if template.properties.len() != result.properties.len()
                || template.call_signatures.len() != result.call_signatures.len()
                || template.construct_signatures.len() != result.construct_signatures.len()
            {
                return Vec::new();
            }
            for (index, (left, right)) in template
                .properties
                .iter()
                .zip(&result.properties)
                .enumerate()
            {
                if left.key != right.key {
                    return Vec::new();
                }
                children.push((DerivationEdge::ObjectProperty(index), left.ty, right.ty));
                if let (Some(left), Some(right)) = (left.write_ty, right.write_ty) {
                    children.push((DerivationEdge::ObjectWriteProperty(index), left, right));
                }
            }
            if let (Some(left), Some(right)) = (template.string_index, result.string_index) {
                children.push((DerivationEdge::ObjectStringIndex, left, right));
            }
            if let (Some(left), Some(right)) = (template.number_index, result.number_index) {
                children.push((DerivationEdge::ObjectNumberIndex, left, right));
            }
            children.extend(
                template
                    .call_signatures
                    .iter()
                    .zip(&result.call_signatures)
                    .enumerate()
                    .map(|(index, (&left, &right))| {
                        (DerivationEdge::ObjectCallSignature(index), left, right)
                    }),
            );
            children.extend(
                template
                    .construct_signatures
                    .iter()
                    .zip(&result.construct_signatures)
                    .enumerate()
                    .map(|(index, (&left, &right))| {
                        (DerivationEdge::ObjectConstructSignature(index), left, right)
                    }),
            );
            children
        }
        TypeTag::Function => {
            let (Some(template), Some(result)) =
                (store.function_type(template), store.function_type(result))
            else {
                return Vec::new();
            };
            let mut children = Vec::new();
            if template.type_params.len() != result.type_params.len()
                || template.params.len() != result.params.len()
            {
                return Vec::new();
            }
            for (index, (left, right)) in template
                .type_params
                .iter()
                .zip(&result.type_params)
                .enumerate()
            {
                if let (Some(left), Some(right)) = (left.constraint, right.constraint) {
                    children.push((DerivationEdge::FunctionConstraint(index), left, right));
                }
                if let (Some(left), Some(right)) = (left.default, right.default) {
                    children.push((DerivationEdge::FunctionDefault(index), left, right));
                }
            }
            children.extend(template.params.iter().zip(&result.params).enumerate().map(
                |(index, (left, right))| {
                    (DerivationEdge::FunctionParameter(index), left.ty, right.ty)
                },
            ));
            if let (Some(left), Some(right)) = (template.receiver, result.receiver) {
                children.push((DerivationEdge::FunctionReceiver, left, right));
            }
            children.push((DerivationEdge::FunctionReturn, template.ret, result.ret));
            children
        }
        TypeTag::Union => canonical_member_children(
            store,
            store.union_members(template).unwrap_or(&[]),
            store.union_members(result).unwrap_or(&[]),
            DerivationEdge::UnionMember,
            mapper,
        ),
        TypeTag::Intersection => canonical_member_children(
            store,
            store.intersection_members(template).unwrap_or(&[]),
            store.intersection_members(result).unwrap_or(&[]),
            DerivationEdge::IntersectionMember,
            mapper,
        ),
        TypeTag::Array => match (store.array_type(template), store.array_type(result)) {
            (Some(left), Some(right)) => {
                vec![(DerivationEdge::ArrayElement, left.element, right.element)]
            }
            _ => Vec::new(),
        },
        TypeTag::Tuple => {
            let (Some(left), Some(right)) = (store.tuple_type(template), store.tuple_type(result))
            else {
                return Vec::new();
            };
            let mut children = positional_children(
                &left.elements,
                &right.elements,
                DerivationEdge::TupleElement,
            );
            if let (Some(left), Some(right)) = (&left.rest, &right.rest) {
                children.push((DerivationEdge::TupleRest, left.ty, right.ty));
            }
            children
        }
        TypeTag::Readonly => match (
            store.readonly_operand(template),
            store.readonly_operand(result),
        ) {
            (Some(left), Some(right)) => vec![(DerivationEdge::ReadonlyOperand, left, right)],
            _ => Vec::new(),
        },
        TypeTag::Conditional => {
            let (Some(left), Some(right)) = (
                store.conditional_type(template),
                store.conditional_type(result),
            ) else {
                return Vec::new();
            };
            vec![
                (DerivationEdge::ConditionalCheck, left.check, right.check),
                (
                    DerivationEdge::ConditionalExtends,
                    left.extends_ty,
                    right.extends_ty,
                ),
                (
                    DerivationEdge::ConditionalTrue,
                    left.true_branch,
                    right.true_branch,
                ),
                (
                    DerivationEdge::ConditionalFalse,
                    left.false_branch,
                    right.false_branch,
                ),
            ]
        }
        TypeTag::Instantiation => {
            let (Some(left), Some(right)) = (
                store.instantiation_type(template),
                store.instantiation_type(result),
            ) else {
                return Vec::new();
            };
            positional_children(
                &left.args.iter().map(|(_, ty)| *ty).collect::<Vec<_>>(),
                &right.args.iter().map(|(_, ty)| *ty).collect::<Vec<_>>(),
                DerivationEdge::InstantiationArgument,
            )
        }
        TypeTag::ClassInstance => {
            let (Some(left), Some(right)) = (
                store.class_instance_type(template),
                store.class_instance_type(result),
            ) else {
                return Vec::new();
            };
            positional_children(&left.args, &right.args, DerivationEdge::ClassArgument)
        }
        TypeTag::Mapped => {
            let (Some(left), Some(right)) =
                (store.mapped_type(template), store.mapped_type(result))
            else {
                return Vec::new();
            };
            let mut children = vec![
                (
                    DerivationEdge::MappedKeySource,
                    left.key_source,
                    right.key_source,
                ),
                (
                    DerivationEdge::MappedValueTemplate,
                    left.value_template,
                    right.value_template,
                ),
            ];
            if let (Some(left), Some(right)) = (left.modifiers_source, right.modifiers_source) {
                children.push((DerivationEdge::MappedModifiersSource, left, right));
            }
            children
        }
        TypeTag::Template => {
            let (Some(left), Some(right)) =
                (store.template_type(template), store.template_type(result))
            else {
                return Vec::new();
            };
            positional_children(&left.holes, &right.holes, DerivationEdge::TemplateHole)
        }
        TypeTag::Keyof => match (store.keyof_operand(template), store.keyof_operand(result)) {
            (Some(left), Some(right)) => vec![(DerivationEdge::KeyofOperand, left, right)],
            _ => Vec::new(),
        },
        TypeTag::DeferredIndexedAccess => match (
            store.deferred_indexed_access_type(template),
            store.deferred_indexed_access_type(result),
        ) {
            (Some(left), Some(right)) => vec![
                (DerivationEdge::DeferredObject, left.object, right.object),
                (DerivationEdge::DeferredIndex, left.index, right.index),
            ],
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

fn canonical_member_children(
    store: &Store,
    templates: &[TypeId],
    results: &[TypeId],
    edge: impl Fn(usize) -> DerivationEdge,
    mapper: &FxHashMap<TypeParamId, DerivedType>,
) -> Vec<(DerivationEdge, TypeId, TypeId)> {
    let mut children = Vec::new();
    let mut claimed = FxHashSet::default();
    for (result_index, &result) in results.iter().enumerate() {
        let candidates = templates
            .iter()
            .enumerate()
            .filter(|(index, template)| {
                !claimed.contains(index) && member_corresponds(store, **template, result, mapper)
            })
            .map(|(index, &template)| (index, template))
            .collect::<Vec<_>>();
        let [(template_index, template)] = candidates.as_slice() else {
            continue;
        };
        claimed.insert(*template_index);
        children.push((edge(result_index), *template, result));
    }
    children
}

fn member_corresponds(
    store: &Store,
    template: TypeId,
    result: TypeId,
    mapper: &FxHashMap<TypeParamId, DerivedType>,
) -> bool {
    if template == result {
        return true;
    }
    if let Some(mapped) = store
        .type_param(template)
        .and_then(|parameter| mapper.get(&parameter.id))
    {
        return mapped.ty == result;
    }
    if store.tag(template) != store.tag(result) {
        return false;
    }
    match store.tag(template) {
        TypeTag::Object => match (store.object_type(template), store.object_type(result)) {
            (Some(left), Some(right)) => {
                left.properties.len() == right.properties.len()
                    && left
                        .properties
                        .iter()
                        .zip(&right.properties)
                        .all(|(left, right)| left.key == right.key)
            }
            _ => false,
        },
        TypeTag::Function => match (store.function_type(template), store.function_type(result)) {
            (Some(left), Some(right)) => left.params.len() == right.params.len(),
            _ => false,
        },
        _ => false,
    }
}

fn positional_children(
    left: &[TypeId],
    right: &[TypeId],
    edge: impl Fn(usize) -> DerivationEdge,
) -> Vec<(DerivationEdge, TypeId, TypeId)> {
    if left.len() != right.len() {
        return Vec::new();
    }
    left.iter()
        .zip(right)
        .enumerate()
        .map(|(index, (&left, &right))| (edge(index), left, right))
        .collect()
}

//! Capability-free operations over immutable product recipes.

use super::project_local_pass_port::{
    number_to_string, well_known_symbol_from_name, Accessibility, DeclarationGroupRecipe,
    DeclarationPayload, GroupKey, LibraryDeclarationProduct, MappedModifier, MemberRecipe,
    ModifierOp, ObjectType, PropertyKey, PropertyNameRecipe, SignatureKey, SymbolicValueNameRecipe,
    TypeParameterKey, TypeRecipeKey, TypeRecipeNode, TypeReferenceResolution,
    ValueReferenceResolution, Visibility,
};
use std::collections::BTreeSet;

#[cfg(test)]
thread_local! {
    static CONTEXTUAL_ORDERED_SEEN_INSERTS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub struct ContextualTypeParameterIndexWorkScopeForTest(u64);

#[cfg(test)]
impl ContextualTypeParameterIndexWorkScopeForTest {
    pub fn start() -> Self {
        Self(CONTEXTUAL_ORDERED_SEEN_INSERTS.get())
    }

    pub fn finish(self) -> u64 {
        CONTEXTUAL_ORDERED_SEEN_INSERTS.get().saturating_sub(self.0)
    }
}

pub(super) const fn mapped_modifier(modifier: MappedModifier) -> ModifierOp {
    match modifier {
        MappedModifier::Preserve => ModifierOp::Keep,
        MappedModifier::Add => ModifierOp::Add,
        MappedModifier::Remove => ModifierOp::Remove,
    }
}

pub(super) const fn member_visibility(accessibility: Option<Accessibility>) -> Visibility {
    match accessibility {
        Some(Accessibility::Protected) => Visibility::Protected,
        Some(Accessibility::Private) => Visibility::Private,
        Some(Accessibility::Public) | None => Visibility::Public,
    }
}

pub(super) fn property_key(
    product: &LibraryDeclarationProduct,
    name: &PropertyNameRecipe,
) -> Option<PropertyKey> {
    match name {
        PropertyNameRecipe::Identifier(name) | PropertyNameRecipe::String(name) => {
            Some(PropertyKey::String(product.strings()[name.index()].clone()))
        }
        PropertyNameRecipe::NumberBits(bits) => {
            Some(PropertyKey::String(number_to_string(f64::from_bits(*bits))))
        }
        PropertyNameRecipe::Private(_) => None,
        PropertyNameRecipe::Computed { value, .. } => match value {
            SymbolicValueNameRecipe::Path {
                path,
                resolution:
                    ValueReferenceResolution::Group(_) | ValueReferenceResolution::IntrinsicGlobalValue,
                ..
            } => path
                .last()
                .and_then(|name| well_known_symbol_from_name(&product.strings()[name.index()]))
                .map(PropertyKey::WellKnownSymbol),
            SymbolicValueNameRecipe::String(name) => {
                Some(PropertyKey::String(product.strings()[name.index()].clone()))
            }
            SymbolicValueNameRecipe::NumberBits(bits) => {
                Some(PropertyKey::String(number_to_string(f64::from_bits(*bits))))
            }
            SymbolicValueNameRecipe::Path {
                resolution: ValueReferenceResolution::Unresolved { .. },
                ..
            } => None,
        },
    }
}

pub(super) fn certified_well_known_symbol_owner(
    product: &LibraryDeclarationProduct,
) -> Option<GroupKey> {
    let mut owner = None;
    for member in product.members() {
        let Some(PropertyNameRecipe::Computed {
            value:
                SymbolicValueNameRecipe::Path {
                    path,
                    resolution: ValueReferenceResolution::Group(candidate),
                    ..
                },
            ..
        }) = member.name.as_ref()
        else {
            continue;
        };
        if path
            .last()
            .and_then(|name| well_known_symbol_from_name(&product.strings()[name.index()]))
            .is_none()
        {
            continue;
        }
        if owner.is_some_and(|owner| owner != *candidate) {
            return None;
        }
        owner = Some(*candidate);
    }
    owner
}

pub(super) fn has_unsupported_computed_name(
    product: &LibraryDeclarationProduct,
    member: &MemberRecipe,
) -> bool {
    matches!(member.name, Some(PropertyNameRecipe::Computed { .. }))
        && member
            .name
            .as_ref()
            .is_some_and(|name| property_key(product, name).is_none())
}

pub(super) fn is_type_parameter_reference(
    product: &LibraryDeclarationProduct,
    recipe: TypeRecipeKey,
) -> bool {
    matches!(
        product.types()[recipe.index()].node,
        TypeRecipeNode::Reference {
            resolution: TypeReferenceResolution::TypeParameter(_),
            ..
        }
    )
}

pub(super) fn build_contextual_type_parameter_index(
    product: &LibraryDeclarationProduct,
) -> Vec<Option<TypeParameterKey>> {
    let mut resolutions = vec![None; product.types().len()];
    for declaration in product.declarations() {
        let mut parameters = Vec::new();
        match &declaration.payload {
            DeclarationPayload::Class {
                type_parameters,
                members,
                heritage,
            }
            | DeclarationPayload::Interface {
                type_parameters,
                members,
                heritage,
            } => {
                parameters.extend(type_parameters.iter().copied());
                let mut seen = BTreeSet::new();
                for recipe in heritage.iter() {
                    assign_contextual_type_parameters(
                        product,
                        *recipe,
                        &parameters,
                        &mut resolutions,
                        &mut seen,
                    );
                }
                for member in members.iter() {
                    assign_member_type_parameters(
                        product,
                        &product.members()[member.index()],
                        &parameters,
                        &mut resolutions,
                        &mut seen,
                    );
                }
            }
            DeclarationPayload::TypeAlias {
                type_parameters,
                value,
            } => {
                parameters.extend(type_parameters.iter().copied());
                assign_contextual_type_parameters(
                    product,
                    *value,
                    &parameters,
                    &mut resolutions,
                    &mut BTreeSet::new(),
                );
            }
            DeclarationPayload::Function { signature } => assign_signature_type_parameters(
                product,
                *signature,
                &parameters,
                &mut resolutions,
                &mut BTreeSet::new(),
            ),
            DeclarationPayload::Variable { annotation }
            | DeclarationPayload::Parameter { annotation }
            | DeclarationPayload::CatchParameter { annotation } => {
                if let Some(recipe) = annotation {
                    assign_contextual_type_parameters(
                        product,
                        *recipe,
                        &parameters,
                        &mut resolutions,
                        &mut BTreeSet::new(),
                    );
                }
            }
            DeclarationPayload::Enum { .. }
            | DeclarationPayload::Namespace { .. }
            | DeclarationPayload::Import
            | DeclarationPayload::ImportEquals
            | DeclarationPayload::NamespaceExport
            | DeclarationPayload::Global { .. }
            | DeclarationPayload::DefaultExportExpression => {}
        }
    }
    resolutions
}

fn assign_signature_type_parameters(
    product: &LibraryDeclarationProduct,
    signature: SignatureKey,
    outer: &[TypeParameterKey],
    resolutions: &mut [Option<TypeParameterKey>],
    seen: &mut BTreeSet<usize>,
) {
    let signature = &product.signatures()[signature.index()];
    let mut parameters = outer.to_vec();
    parameters.extend(signature.type_parameters.iter().copied());
    for parameter in signature.type_parameters.iter() {
        let parameter = &product.type_parameters()[parameter.index()];
        for recipe in parameter.constraint.iter().chain(parameter.default.iter()) {
            assign_contextual_type_parameters(product, *recipe, &parameters, resolutions, seen);
        }
    }
    for parameter in signature.parameters.iter() {
        if let Some(recipe) = parameter.value {
            assign_contextual_type_parameters(product, recipe, &parameters, resolutions, seen);
        }
    }
    if let Some(recipe) = signature.result {
        assign_contextual_type_parameters(product, recipe, &parameters, resolutions, seen);
    }
}

fn assign_member_type_parameters(
    product: &LibraryDeclarationProduct,
    member: &MemberRecipe,
    parameters: &[TypeParameterKey],
    resolutions: &mut [Option<TypeParameterKey>],
    seen: &mut BTreeSet<usize>,
) {
    if let Some(recipe) = member.value {
        assign_contextual_type_parameters(product, recipe, parameters, resolutions, seen);
    }
    if let Some(signature) = member.signature {
        assign_signature_type_parameters(product, signature, parameters, resolutions, seen);
    }
}

fn assign_contextual_type_parameters(
    product: &LibraryDeclarationProduct,
    recipe: TypeRecipeKey,
    parameters: &[TypeParameterKey],
    resolutions: &mut [Option<TypeParameterKey>],
    seen: &mut BTreeSet<usize>,
) {
    #[cfg(test)]
    CONTEXTUAL_ORDERED_SEEN_INSERTS.set(CONTEXTUAL_ORDERED_SEEN_INSERTS.get() + 1);
    if !seen.insert(recipe.index()) {
        return;
    }
    match &product.types()[recipe.index()].node {
        TypeRecipeNode::Reference {
            path,
            resolution: TypeReferenceResolution::Unresolved { .. },
            arguments,
            ..
        } => {
            if let [name] = path.as_ref() {
                resolutions[recipe.index()] =
                    parameters.iter().rev().copied().find(|parameter| {
                        product.type_parameters()[parameter.index()].name == *name
                    });
            }
            for child in arguments.iter() {
                assign_contextual_type_parameters(product, *child, parameters, resolutions, seen);
            }
        }
        TypeRecipeNode::Reference { arguments, .. }
        | TypeRecipeNode::Import { arguments, .. }
        | TypeRecipeNode::Union(arguments)
        | TypeRecipeNode::Intersection(arguments) => {
            for child in arguments.iter() {
                assign_contextual_type_parameters(product, *child, parameters, resolutions, seen);
            }
        }
        TypeRecipeNode::Array(child) | TypeRecipeNode::Operator { operand: child, .. } => {
            assign_contextual_type_parameters(product, *child, parameters, resolutions, seen);
        }
        TypeRecipeNode::Tuple { elements } => {
            for child in elements.iter() {
                assign_contextual_type_parameters(
                    product,
                    child.value,
                    parameters,
                    resolutions,
                    seen,
                );
            }
        }
        TypeRecipeNode::Function(signature) | TypeRecipeNode::Constructor { signature, .. } => {
            assign_signature_type_parameters(product, *signature, parameters, resolutions, seen);
        }
        TypeRecipeNode::Object(members) => {
            for member in members.iter() {
                assign_member_type_parameters(
                    product,
                    &product.members()[member.index()],
                    parameters,
                    resolutions,
                    seen,
                );
            }
        }
        TypeRecipeNode::IndexedAccess { object, index } => {
            assign_contextual_type_parameters(product, *object, parameters, resolutions, seen);
            assign_contextual_type_parameters(product, *index, parameters, resolutions, seen);
        }
        TypeRecipeNode::Mapped {
            parameter,
            name,
            value,
            ..
        } => {
            let mut mapped_parameters = parameters.to_vec();
            mapped_parameters.push(*parameter);
            for child in name.iter().chain(value.iter()) {
                assign_contextual_type_parameters(
                    product,
                    *child,
                    &mapped_parameters,
                    resolutions,
                    seen,
                );
            }
        }
        TypeRecipeNode::Conditional {
            check,
            extends,
            when_true,
            when_false,
        } => {
            for child in [check, extends, when_true, when_false] {
                assign_contextual_type_parameters(product, *child, parameters, resolutions, seen);
            }
        }
        TypeRecipeNode::Template { holes, .. } => {
            for child in holes.iter() {
                assign_contextual_type_parameters(product, *child, parameters, resolutions, seen);
            }
        }
        TypeRecipeNode::Predicate { value, .. } => {
            if let Some(child) = value {
                assign_contextual_type_parameters(product, *child, parameters, resolutions, seen);
            }
        }
        TypeRecipeNode::Keyword(_)
        | TypeRecipeNode::Literal(_)
        | TypeRecipeNode::Query { .. }
        | TypeRecipeNode::Infer(_) => {}
    }
}

pub(super) fn group_has_runtime_value(
    product: &LibraryDeclarationProduct,
    group: &DeclarationGroupRecipe,
) -> bool {
    group.fragments.iter().any(|declaration| {
        matches!(
            product.declarations()[declaration.index()].payload,
            DeclarationPayload::Variable { .. }
                | DeclarationPayload::Function { .. }
                | DeclarationPayload::Class { .. }
                | DeclarationPayload::Enum { .. }
                | DeclarationPayload::Namespace { .. }
                | DeclarationPayload::Global { .. }
        )
    })
}

pub(super) fn merge_object_surface(target: &mut ObjectType, source: ObjectType) {
    for property in source.properties {
        if target
            .properties
            .iter()
            .all(|existing| existing.key != property.key)
        {
            target.properties.push(property);
        }
    }
    target.string_index = target.string_index.or(source.string_index);
    target.number_index = target.number_index.or(source.number_index);
    target.call_signatures.extend(source.call_signatures);
    target
        .construct_signatures
        .extend(source.construct_signatures);
}

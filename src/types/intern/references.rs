//! Typed reference projection for the interner and its type store.
//!
//! Enumerates every identity edge the frozen universe holds as `(owner_domain,
//! target_domain, field, owner, target)` rows. Replay-index generation consumes the
//! store rows; the reference-integrity specs consume the whole projection. No bytes
//! are produced — this is a traversal, not a format.

use super::*;
use crate::types::repr::DeclaredRecipeNode;

#[cfg(test)]
const WELL_KNOWN_COUNT: usize = 17;

// Reference-record domains. These discriminants are append-only.
const CONTAINER_DOMAIN: u8 = 0;
const TYPE_DOMAIN: u8 = 1;
const TYPE_PARAM_DOMAIN: u8 = 2;
const CLASS_DOMAIN: u8 = 3;
const DECLARED_RECIPE_DOMAIN: u8 = 10;
#[cfg(test)]
const INTERNER_BUCKET_DOMAIN: u8 = 16;

// Store TypeId owners reuse these relationship fields across payload kinds.
const TYPE_OPERAND_FIELD: u8 = 0;
const TYPE_PARAM_IDENTITY_FIELD: u8 = 1;
const CLASS_IDENTITY_FIELD: u8 = 2;
const CONSTRAINT_FIELD: u8 = 3;
const DEFAULT_FIELD: u8 = 4;
const DECLARING_CLASS_FIELD: u8 = 5;
const DECLARED_RECIPE_FIELD: u8 = 6;

// Declared-recipe rows are identities separate from the TypeId arena.
const RECIPE_TYPE_FIELD: u8 = 0;
const RECIPE_PARAMETER_FIELD: u8 = 1;
const RECIPE_CHILD_FIELD: u8 = 2;
const RECIPE_FREE_PARAMETER_FIELD: u8 = 3;
const ROW_IDENTITY_FIELD: u8 = 31;

// Store metadata container fields.
const CONSTRAINT_OWNER_FIELD: u8 = 6;
const CONSTRAINT_TARGET_FIELD: u8 = 7;
const FROZEN_TYPE_PARAM_FIELD: u8 = 8;
const TEMPLATE_NAME_TYPE_FIELD: u8 = 9;

#[cfg(test)]
// Interner identity container fields.
const BUCKET_CANDIDATE_FIELD: u8 = 0;
#[cfg(test)]
const RESERVED_TYPE_FIELD: u8 = 1;
#[cfg(test)]
const WELL_KNOWN_TYPE_FIELD: u8 = 2;

pub(crate) type ReferenceRecord = (u8, u8, u8, u32, u32);

fn reference(
    owner_domain: u8,
    target_domain: u8,
    field: u8,
    owner: u32,
    target: u32,
) -> ReferenceRecord {
    (owner_domain, target_domain, field, owner, target)
}

fn push_type_operand(references: &mut Vec<ReferenceRecord>, owner: TypeId, target: TypeId) {
    references.push(reference(
        TYPE_DOMAIN,
        TYPE_DOMAIN,
        TYPE_OPERAND_FIELD,
        owner.0,
        target.0,
    ));
}

fn push_type_param_identity(
    references: &mut Vec<ReferenceRecord>,
    owner: TypeId,
    target: TypeParamId,
) {
    references.push(reference(
        TYPE_DOMAIN,
        TYPE_PARAM_DOMAIN,
        TYPE_PARAM_IDENTITY_FIELD,
        owner.0,
        target.0,
    ));
}

fn push_class_identity(
    references: &mut Vec<ReferenceRecord>,
    owner: TypeId,
    field: u8,
    target: crate::types::repr::ClassId,
) {
    references.push(reference(
        TYPE_DOMAIN,
        CLASS_DOMAIN,
        field,
        owner.0,
        target.0,
    ));
}

impl Interner {
    pub(crate) fn typed_reference_records_for_replay_generation(
        &self,
    ) -> Result<Vec<ReferenceRecord>, &'static str> {
        self.store_reference_records()
    }

    #[cfg(test)]
    /// Canonical reference rows for the store rows and for the interner's own identity
    /// tables, in that order.
    ///
    /// Tuple order is `(owner_domain, target_domain, field, owner, target)`.
    pub(crate) fn reference_records(
        &self,
    ) -> Result<(Vec<ReferenceRecord>, Vec<ReferenceRecord>), &'static str> {
        if self.has_nonempty_delta() {
            return Err("reference projection requires an interner with an empty delta");
        }
        self.reference_records_for_complete_state()
    }

    #[cfg(test)]
    fn reference_records_for_complete_state(
        &self,
    ) -> Result<(Vec<ReferenceRecord>, Vec<ReferenceRecord>), &'static str> {
        let store_references = self.store_reference_records()?;
        let mut interner_references = Vec::new();

        let mut buckets = self.dedup_buckets().collect::<Vec<_>>();
        buckets.sort_unstable_by_key(|(hash, _)| **hash);
        for (bucket_index, (_, candidates)) in buckets.into_iter().enumerate() {
            let owner =
                u32::try_from(bucket_index).map_err(|_| "dedup bucket index exceeds u32")?;
            let mut candidates = candidates.iter().copied().collect::<Vec<_>>();
            candidates.sort_unstable();
            interner_references.extend(candidates.into_iter().map(|candidate| {
                reference(
                    INTERNER_BUCKET_DOMAIN,
                    TYPE_DOMAIN,
                    BUCKET_CANDIDATE_FIELD,
                    owner,
                    candidate.0,
                )
            }));
        }

        let mut reserved = self.reserved_types().map(|(&id, _)| id).collect::<Vec<_>>();
        reserved.sort_unstable();
        for (index, id) in reserved.into_iter().enumerate() {
            interner_references.push(reference(
                CONTAINER_DOMAIN,
                TYPE_DOMAIN,
                RESERVED_TYPE_FIELD,
                u32::try_from(index).map_err(|_| "reserved type index exceeds u32")?,
                id.0,
            ));
        }

        for (slot, id) in well_known_ids(self.well_known).into_iter().enumerate() {
            interner_references.push(reference(
                CONTAINER_DOMAIN,
                TYPE_DOMAIN,
                WELL_KNOWN_TYPE_FIELD,
                u32::try_from(slot).map_err(|_| "well-known slot exceeds u32")?,
                id.0,
            ));
        }

        interner_references.sort_unstable();
        Ok((store_references, interner_references))
    }

    #[cfg(test)]
    pub(crate) fn reference_records_for_test(
        &self,
    ) -> (Vec<ReferenceRecord>, Vec<ReferenceRecord>) {
        self.reference_records()
            .expect("typed interner references enumerate")
    }

    #[cfg(test)]
    pub(crate) fn local_type_reference_records_for_test(&self) -> Vec<ReferenceRecord> {
        let mut records = self
            .store_reference_records_from(self.store.frozen_prefix_len_for_test(), true)
            .expect("typed local store references enumerate");
        for (&hash, candidates) in &self.dedup {
            let owner = u32::try_from(hash).unwrap_or(u32::MAX);
            records.extend(candidates.iter().map(|candidate| {
                reference(
                    INTERNER_BUCKET_DOMAIN,
                    TYPE_DOMAIN,
                    BUCKET_CANDIDATE_FIELD,
                    owner,
                    candidate.0,
                )
            }));
        }
        records.extend(self.reserved_types.keys().map(|id| {
            reference(
                CONTAINER_DOMAIN,
                TYPE_DOMAIN,
                RESERVED_TYPE_FIELD,
                id.0,
                id.0,
            )
        }));
        records
    }

    fn store_reference_records(&self) -> Result<Vec<ReferenceRecord>, &'static str> {
        self.store_reference_records_from(0, false)
    }

    fn store_reference_records_from(
        &self,
        start: usize,
        local_side_columns: bool,
    ) -> Result<Vec<ReferenceRecord>, &'static str> {
        let store = &self.store;
        let mut references = Vec::new();
        let recipe_start = if local_side_columns {
            store.declared_recipe_base_len()
        } else {
            0
        };
        for (recipe_id, recipe) in store.all_declared_recipes().skip(recipe_start) {
            references.push(reference(
                DECLARED_RECIPE_DOMAIN,
                DECLARED_RECIPE_DOMAIN,
                ROW_IDENTITY_FIELD,
                recipe_id.0,
                recipe_id.0,
            ));
            match &recipe.node {
                DeclaredRecipeNode::Type(ty) => references.push(reference(
                    DECLARED_RECIPE_DOMAIN,
                    TYPE_DOMAIN,
                    RECIPE_TYPE_FIELD,
                    recipe_id.0,
                    ty.0,
                )),
                DeclaredRecipeNode::Array(child) | DeclaredRecipeNode::Readonly(child) => {
                    references.push(reference(
                        DECLARED_RECIPE_DOMAIN,
                        DECLARED_RECIPE_DOMAIN,
                        RECIPE_CHILD_FIELD,
                        recipe_id.0,
                        child.0,
                    ))
                }
                DeclaredRecipeNode::Tuple { elements, rest } => {
                    references.extend(elements.iter().map(|child| {
                        reference(
                            DECLARED_RECIPE_DOMAIN,
                            DECLARED_RECIPE_DOMAIN,
                            RECIPE_CHILD_FIELD,
                            recipe_id.0,
                            child.0,
                        )
                    }));
                    if let Some((_, child)) = rest {
                        references.push(reference(
                            DECLARED_RECIPE_DOMAIN,
                            DECLARED_RECIPE_DOMAIN,
                            RECIPE_CHILD_FIELD,
                            recipe_id.0,
                            child.0,
                        ));
                    }
                }
                DeclaredRecipeNode::Application {
                    template,
                    parameters,
                    arguments,
                } => {
                    references.push(reference(
                        DECLARED_RECIPE_DOMAIN,
                        TYPE_DOMAIN,
                        RECIPE_TYPE_FIELD,
                        recipe_id.0,
                        template.0,
                    ));
                    references.extend(parameters.iter().map(|parameter| {
                        reference(
                            DECLARED_RECIPE_DOMAIN,
                            TYPE_PARAM_DOMAIN,
                            RECIPE_PARAMETER_FIELD,
                            recipe_id.0,
                            parameter.0,
                        )
                    }));
                    references.extend(arguments.iter().map(|child| {
                        reference(
                            DECLARED_RECIPE_DOMAIN,
                            DECLARED_RECIPE_DOMAIN,
                            RECIPE_CHILD_FIELD,
                            recipe_id.0,
                            child.0,
                        )
                    }));
                }
            }
            references.extend(recipe.free_params.iter().map(|parameter| {
                reference(
                    DECLARED_RECIPE_DOMAIN,
                    TYPE_PARAM_DOMAIN,
                    RECIPE_FREE_PARAMETER_FIELD,
                    recipe_id.0,
                    parameter.0,
                )
            }));
        }
        for raw_owner in start..store.len() {
            let owner = TypeId(u32::try_from(raw_owner).map_err(|_| "TypeId exceeds u32")?);

            match store.tag(owner) {
                TypeTag::Intrinsic | TypeTag::Literal | TypeTag::Infer | TypeTag::MappedValue => {}
                TypeTag::Object => {
                    let object = store
                        .object_type(owner)
                        .ok_or("validated object payload is missing")?;
                    for property in &object.properties {
                        push_type_operand(&mut references, owner, property.ty);
                        if let Some(write_ty) = property.write_ty {
                            push_type_operand(&mut references, owner, write_ty);
                        }
                        if let Some(class) = property.declaring_class {
                            push_class_identity(
                                &mut references,
                                owner,
                                DECLARING_CLASS_FIELD,
                                class,
                            );
                        }
                    }
                    if let Some(index) = object.string_index {
                        push_type_operand(&mut references, owner, index);
                    }
                    if let Some(index) = object.number_index {
                        push_type_operand(&mut references, owner, index);
                    }
                    for &signature in &object.call_signatures {
                        push_type_operand(&mut references, owner, signature);
                    }
                    for &signature in &object.construct_signatures {
                        push_type_operand(&mut references, owner, signature);
                    }
                }
                TypeTag::Union => {
                    for &member in store
                        .union_members(owner)
                        .ok_or("validated union payload is missing")?
                    {
                        push_type_operand(&mut references, owner, member);
                    }
                }
                TypeTag::Intersection => {
                    for &member in store
                        .intersection_members(owner)
                        .ok_or("validated intersection payload is missing")?
                    {
                        push_type_operand(&mut references, owner, member);
                    }
                }
                TypeTag::Function => {
                    let function = store
                        .function_type(owner)
                        .ok_or("validated function payload is missing")?;
                    for parameter in &function.type_params {
                        push_type_param_identity(&mut references, owner, parameter.id);
                        if let Some(constraint) = parameter.constraint {
                            references.push(reference(
                                TYPE_DOMAIN,
                                TYPE_DOMAIN,
                                CONSTRAINT_FIELD,
                                owner.0,
                                constraint.0,
                            ));
                        }
                        if let Some(default) = parameter.default {
                            references.push(reference(
                                TYPE_DOMAIN,
                                TYPE_DOMAIN,
                                DEFAULT_FIELD,
                                owner.0,
                                default.0,
                            ));
                        }
                    }
                    if let Some(receiver) = function.receiver {
                        push_type_operand(&mut references, owner, receiver);
                    }
                    for parameter in &function.params {
                        push_type_operand(&mut references, owner, parameter.ty);
                    }
                    push_type_operand(&mut references, owner, function.ret);
                }
                TypeTag::TypeParam => {
                    push_type_param_identity(
                        &mut references,
                        owner,
                        store
                            .type_param(owner)
                            .ok_or("validated type parameter payload is missing")?
                            .id,
                    );
                }
                TypeTag::Array => {
                    push_type_operand(
                        &mut references,
                        owner,
                        store
                            .array_type(owner)
                            .ok_or("validated array payload is missing")?
                            .element,
                    );
                }
                TypeTag::Tuple => {
                    let tuple = store
                        .tuple_type(owner)
                        .ok_or("validated tuple payload is missing")?;
                    for &element in &tuple.elements {
                        push_type_operand(&mut references, owner, element);
                    }
                    if let Some(rest) = tuple.rest {
                        push_type_operand(&mut references, owner, rest.ty);
                    }
                }
                TypeTag::Readonly => {
                    push_type_operand(
                        &mut references,
                        owner,
                        store
                            .readonly_operand(owner)
                            .ok_or("validated readonly payload is missing")?,
                    );
                }
                TypeTag::Conditional => {
                    let conditional = store
                        .conditional_type(owner)
                        .ok_or("validated conditional payload is missing")?;
                    for target in [
                        conditional.check,
                        conditional.extends_ty,
                        conditional.true_branch,
                        conditional.false_branch,
                    ] {
                        push_type_operand(&mut references, owner, target);
                    }
                }
                TypeTag::Instantiation => {
                    let instantiation = store
                        .instantiation_type(owner)
                        .ok_or("validated instantiation payload is missing")?;
                    push_type_operand(&mut references, owner, instantiation.base);
                    for &(parameter, argument) in &instantiation.args {
                        push_type_param_identity(&mut references, owner, parameter);
                        push_type_operand(&mut references, owner, argument);
                    }
                }
                TypeTag::Mapped => {
                    let mapped = store
                        .mapped_type(owner)
                        .ok_or("validated mapped payload is missing")?;
                    push_type_operand(&mut references, owner, mapped.key_source);
                    push_type_operand(&mut references, owner, mapped.value_template);
                    if let Some(source) = mapped.modifiers_source {
                        push_type_operand(&mut references, owner, source);
                    }
                }
                TypeTag::Template => {
                    for &hole in &store
                        .template_type(owner)
                        .ok_or("validated template payload is missing")?
                        .holes
                    {
                        push_type_operand(&mut references, owner, hole);
                    }
                }
                TypeTag::Keyof => {
                    push_type_operand(
                        &mut references,
                        owner,
                        store
                            .keyof_operand(owner)
                            .ok_or("validated keyof payload is missing")?,
                    );
                }
                TypeTag::ClassInstance => {
                    let instance = store
                        .class_instance_type(owner)
                        .ok_or("validated class instance payload is missing")?;
                    push_class_identity(
                        &mut references,
                        owner,
                        CLASS_IDENTITY_FIELD,
                        instance.class,
                    );
                    for &argument in &instance.args {
                        push_type_operand(&mut references, owner, argument);
                    }
                }
                TypeTag::DeferredIndexedAccess => {
                    let access = store
                        .deferred_indexed_access_type(owner)
                        .ok_or("validated deferred indexed access payload is missing")?;
                    push_type_operand(&mut references, owner, access.object);
                    push_type_operand(&mut references, owner, access.index);
                }
                TypeTag::Declared => {
                    let declared = store
                        .declared_type(owner)
                        .ok_or("validated declared payload is missing")?;
                    references.push(reference(
                        TYPE_DOMAIN,
                        DECLARED_RECIPE_DOMAIN,
                        DECLARED_RECIPE_FIELD,
                        owner.0,
                        declared.recipe.0,
                    ));
                    for &(parameter, value) in &declared.mapper {
                        push_type_param_identity(&mut references, owner, parameter);
                        push_type_operand(&mut references, owner, value);
                    }
                }
            }
        }

        let constraints = if local_side_columns {
            store
                .local_type_param_constraints_for_test()
                .collect::<Vec<_>>()
        } else {
            store.all_type_param_constraints()
        };
        for (index, (parameter, constraint)) in constraints.into_iter().enumerate() {
            let owner = u32::try_from(index).map_err(|_| "constraint row index exceeds u32")?;
            references.push(reference(
                CONTAINER_DOMAIN,
                TYPE_PARAM_DOMAIN,
                CONSTRAINT_OWNER_FIELD,
                owner,
                parameter.0,
            ));
            references.push(reference(
                CONTAINER_DOMAIN,
                TYPE_DOMAIN,
                CONSTRAINT_TARGET_FIELD,
                owner,
                constraint.0,
            ));
        }
        let frozen_parameters = if local_side_columns {
            store
                .local_frozen_type_params_for_test()
                .collect::<Vec<_>>()
        } else {
            store.all_frozen_type_params()
        };
        for (index, parameter) in frozen_parameters.into_iter().enumerate() {
            references.push(reference(
                CONTAINER_DOMAIN,
                TYPE_PARAM_DOMAIN,
                FROZEN_TYPE_PARAM_FIELD,
                u32::try_from(index).map_err(|_| "frozen parameter index exceeds u32")?,
                parameter.0,
            ));
        }
        let mut template_names = if local_side_columns {
            store.local_template_name_ids_for_test().collect::<Vec<_>>()
        } else {
            store.all_template_name_ids().collect::<Vec<_>>()
        };
        template_names.sort_unstable();
        for (index, id) in template_names.into_iter().enumerate() {
            references.push(reference(
                CONTAINER_DOMAIN,
                TYPE_DOMAIN,
                TEMPLATE_NAME_TYPE_FIELD,
                u32::try_from(index).map_err(|_| "template-name row index exceeds u32")?,
                id.0,
            ));
        }

        references.sort_unstable();
        Ok(references)
    }
}

#[cfg(test)]
fn well_known_ids(well_known: WellKnown) -> [TypeId; WELL_KNOWN_COUNT] {
    [
        well_known.error,
        well_known.any,
        well_known.unknown,
        well_known.never,
        well_known.void,
        well_known.null,
        well_known.undefined,
        well_known.boolean,
        well_known.number,
        well_known.string,
        well_known.uppercase,
        well_known.lowercase,
        well_known.capitalize,
        well_known.uncapitalize,
        well_known.this_type,
        well_known.omit_this_parameter,
        well_known.object,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::repr::{
        ClassId, ConditionalType, DeclaredRecipeNode, FunctionType, GenericTypeParam, MappedType,
        ModifierOp, ObjectType, ParameterType, PropertyType, TemplateType, TupleRestType,
        TupleType, Visibility,
    };
    use crate::types::substitute::SubstitutionOutcome;

    struct RichFixture {
        interner: Interner,
        literal: TypeId,
        parameter_id: TypeParamId,
        parameter: TypeId,
        array: TypeId,
        readonly: TypeId,
        tuple: TypeId,
        function: TypeId,
        object: TypeId,
        conditional: TypeId,
        instantiation: TypeId,
        mapped_value: TypeId,
        mapped: TypeId,
        template: TypeId,
        union: TypeId,
        intersection: TypeId,
        keyof: TypeId,
        class_instance: TypeId,
        indexed: TypeId,
    }

    fn rich_fixture() -> RichFixture {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let literal = interner.intern_literal(LiteralValue::String("fixture".to_owned()));
        let _number_literal = interner.intern_literal(LiteralValue::Number(-0.0));
        let _boolean_literal = interner.intern_literal(LiteralValue::Boolean(true));
        let parameter_id = TypeParamId(7);
        let parameter = interner.intern_type_param(parameter_id, "T");
        assert!(interner.set_type_param_constraint(parameter_id, wk.string));
        interner
            .freeze_type_param_metadata(&[parameter_id])
            .expect("fresh parameter freezes");
        let array = interner.intern_array(parameter);
        let readonly = interner.intern_readonly(array);
        let tuple = interner.intern_tuple_type(TupleType::with_rest(
            vec![literal],
            TupleRestType::new(1, array),
        ));
        let function = interner.intern_function(FunctionType {
            type_params: vec![GenericTypeParam {
                id: parameter_id,
                constraint: Some(wk.string),
                default: Some(literal),
            }],
            receiver: Some(parameter),
            params: vec![ParameterType::required("value", tuple)],
            ret: wk.boolean,
        });
        let object = interner.reserve_object();
        let class_property = PropertyType {
            name: "classy".to_owned(),
            ty: literal,
            write_ty: Some(wk.string),
            optional: false,
            visibility: Visibility::Protected,
            declaring_class: Some(ClassId(12)),
            readonly: true,
            is_accessor: true,
        };
        interner.fill_object(
            object,
            ObjectType {
                properties: vec![class_property, PropertyType::public("next", object)],
                string_index: Some(literal),
                number_index: Some(wk.number),
                call_signatures: vec![function],
                construct_signatures: vec![function],
            },
        );
        let conditional = interner.reserve_conditional();
        interner.fill_conditional(
            conditional,
            ConditionalType {
                check: parameter,
                extends_ty: wk.string,
                true_branch: object,
                false_branch: wk.never,
                infer_count: 0,
                distributive: true,
                poisoned: false,
            },
        );
        interner.set_template_name(conditional, "FixtureConditional");
        let instantiation =
            interner.intern_instantiation(conditional, vec![(parameter_id, literal)]);
        let _infer = interner.intern_infer(0);
        let mapped_value = interner.intern_mapped_value();
        let mapped = interner.reserve_mapped();
        interner.fill_mapped(
            mapped,
            MappedType {
                homomorphic: true,
                key_source: wk.string,
                value_template: mapped_value,
                modifiers_source: Some(object),
                optional_modifier: ModifierOp::Add,
                readonly_modifier: ModifierOp::Remove,
            },
        );
        interner.set_template_name(mapped, "FixtureMapped");
        let template = interner.intern_template(TemplateType {
            texts: vec!["before".to_owned(), "after".to_owned()],
            holes: vec![literal],
        });
        let union = interner.union(vec![literal, wk.string]);
        let intersection = interner.intersection(vec![object, mapped]);
        let keyof = interner.intern_keyof(parameter);
        let class_instance = interner.intern_class_instance(ClassId(11), vec![literal]);
        let indexed = interner.intern_deferred_indexed_access(class_instance, parameter);
        RichFixture {
            interner,
            literal,
            parameter_id,
            parameter,
            array,
            readonly,
            tuple,
            function,
            object,
            conditional,
            instantiation,
            mapped_value,
            mapped,
            template,
            union,
            intersection,
            keyof,
            class_instance,
            indexed,
        }
    }

    #[test]
    fn declared_recipe_references_enumerate_and_materialize_in_an_isolated_suffix() {
        let mut interner = Interner::with_intrinsics();
        let parameter_id = TypeParamId(41);
        let parameter = interner.intern_type_param(parameter_id, "T");
        let leaf = interner.intern_declared_recipe(DeclaredRecipeNode::Type(parameter));
        let array = interner.intern_declared_recipe(DeclaredRecipeNode::Array(leaf));
        let application = interner.intern_declared_recipe(DeclaredRecipeNode::Application {
            template: parameter,
            parameters: vec![parameter_id],
            arguments: vec![array],
        });
        let string = interner.well_known().string;
        let declared = interner.intern_declared(array, [(parameter_id, string)]);
        let recipe_records = interner
            .reference_records_for_test()
            .0
            .into_iter()
            .filter(|record| {
                record.0 == DECLARED_RECIPE_DOMAIN || record.1 == DECLARED_RECIPE_DOMAIN
            })
            .collect::<Vec<_>>();
        assert_eq!(
            recipe_records,
            vec![
                reference(
                    TYPE_DOMAIN,
                    DECLARED_RECIPE_DOMAIN,
                    DECLARED_RECIPE_FIELD,
                    declared.0,
                    array.0,
                ),
                reference(
                    DECLARED_RECIPE_DOMAIN,
                    TYPE_DOMAIN,
                    RECIPE_TYPE_FIELD,
                    leaf.0,
                    parameter.0,
                ),
                reference(
                    DECLARED_RECIPE_DOMAIN,
                    TYPE_DOMAIN,
                    RECIPE_TYPE_FIELD,
                    application.0,
                    parameter.0,
                ),
                reference(
                    DECLARED_RECIPE_DOMAIN,
                    TYPE_PARAM_DOMAIN,
                    RECIPE_PARAMETER_FIELD,
                    application.0,
                    parameter_id.0,
                ),
                reference(
                    DECLARED_RECIPE_DOMAIN,
                    TYPE_PARAM_DOMAIN,
                    RECIPE_FREE_PARAMETER_FIELD,
                    leaf.0,
                    parameter_id.0,
                ),
                reference(
                    DECLARED_RECIPE_DOMAIN,
                    TYPE_PARAM_DOMAIN,
                    RECIPE_FREE_PARAMETER_FIELD,
                    array.0,
                    parameter_id.0,
                ),
                reference(
                    DECLARED_RECIPE_DOMAIN,
                    TYPE_PARAM_DOMAIN,
                    RECIPE_FREE_PARAMETER_FIELD,
                    application.0,
                    parameter_id.0,
                ),
                reference(
                    DECLARED_RECIPE_DOMAIN,
                    DECLARED_RECIPE_DOMAIN,
                    RECIPE_CHILD_FIELD,
                    array.0,
                    leaf.0,
                ),
                reference(
                    DECLARED_RECIPE_DOMAIN,
                    DECLARED_RECIPE_DOMAIN,
                    RECIPE_CHILD_FIELD,
                    application.0,
                    array.0,
                ),
                reference(
                    DECLARED_RECIPE_DOMAIN,
                    DECLARED_RECIPE_DOMAIN,
                    ROW_IDENTITY_FIELD,
                    leaf.0,
                    leaf.0,
                ),
                reference(
                    DECLARED_RECIPE_DOMAIN,
                    DECLARED_RECIPE_DOMAIN,
                    ROW_IDENTITY_FIELD,
                    array.0,
                    array.0,
                ),
                reference(
                    DECLARED_RECIPE_DOMAIN,
                    DECLARED_RECIPE_DOMAIN,
                    ROW_IDENTITY_FIELD,
                    application.0,
                    application.0,
                ),
            ]
        );

        let expected_array = interner.intern_array(string);
        assert_eq!(
            interner.materialize_declared(declared),
            Some(SubstitutionOutcome::CycleClean(expected_array))
        );

        interner.freeze_as_base().expect("recipe base freezes");
        let mut fork = interner.fork_delta().expect("recipe base forks");
        let materialized = fork
            .materialize_declared(declared)
            .expect("base recipe materializes");
        let SubstitutionOutcome::CycleClean(materialized) = materialized else {
            panic!("acyclic declared recipe materializes cycle-clean");
        };
        assert_eq!(
            fork.store()
                .array_type(materialized)
                .map(|array| array.element),
            Some(fork.well_known().string)
        );
        assert!(interner.store().shares_base_rows_with(fork.store()));
        assert_eq!(interner.base_index_family_sharing_with(&fork), [true; 4]);
        let string = fork.well_known().string;
        fork.intern_declared_recipe(DeclaredRecipeNode::Type(string));
        assert_eq!(fork.local_index_row_counts_for_test(), [0, 0, 1, 0]);
    }

    fn rich_interner() -> Interner {
        rich_fixture().interner
    }

    #[test]
    fn reference_manifest_exactly_enumerates_rich_type_universe() {
        let fixture = rich_fixture();
        let wk = fixture.interner.well_known();
        let (store_references, interner_references) = fixture.interner.reference_records_for_test();

        let type_ref = |owner: TypeId, target: TypeId| {
            reference(
                TYPE_DOMAIN,
                TYPE_DOMAIN,
                TYPE_OPERAND_FIELD,
                owner.0,
                target.0,
            )
        };
        let parameter_ref = |owner: TypeId, target: TypeParamId| {
            reference(
                TYPE_DOMAIN,
                TYPE_PARAM_DOMAIN,
                TYPE_PARAM_IDENTITY_FIELD,
                owner.0,
                target.0,
            )
        };
        let mut expected_store = vec![
            parameter_ref(fixture.parameter, fixture.parameter_id),
            type_ref(fixture.array, fixture.parameter),
            type_ref(fixture.readonly, fixture.array),
            type_ref(fixture.tuple, fixture.literal),
            type_ref(fixture.tuple, fixture.array),
            parameter_ref(fixture.function, fixture.parameter_id),
            reference(
                TYPE_DOMAIN,
                TYPE_DOMAIN,
                CONSTRAINT_FIELD,
                fixture.function.0,
                wk.string.0,
            ),
            reference(
                TYPE_DOMAIN,
                TYPE_DOMAIN,
                DEFAULT_FIELD,
                fixture.function.0,
                fixture.literal.0,
            ),
            type_ref(fixture.function, fixture.parameter),
            type_ref(fixture.function, fixture.tuple),
            type_ref(fixture.function, wk.boolean),
            type_ref(fixture.object, fixture.literal),
            type_ref(fixture.object, wk.string),
            reference(
                TYPE_DOMAIN,
                CLASS_DOMAIN,
                DECLARING_CLASS_FIELD,
                fixture.object.0,
                12,
            ),
            type_ref(fixture.object, fixture.object),
            type_ref(fixture.object, fixture.literal),
            type_ref(fixture.object, wk.number),
            type_ref(fixture.object, fixture.function),
            type_ref(fixture.object, fixture.function),
            type_ref(fixture.conditional, fixture.parameter),
            type_ref(fixture.conditional, wk.string),
            type_ref(fixture.conditional, fixture.object),
            type_ref(fixture.conditional, wk.never),
            type_ref(fixture.instantiation, fixture.conditional),
            parameter_ref(fixture.instantiation, fixture.parameter_id),
            type_ref(fixture.instantiation, fixture.literal),
            type_ref(fixture.mapped, wk.string),
            type_ref(fixture.mapped, fixture.mapped_value),
            type_ref(fixture.mapped, fixture.object),
            type_ref(fixture.template, fixture.literal),
            type_ref(fixture.union, fixture.literal),
            type_ref(fixture.union, wk.string),
            type_ref(fixture.intersection, fixture.object),
            type_ref(fixture.intersection, fixture.mapped),
            type_ref(fixture.keyof, fixture.parameter),
            reference(
                TYPE_DOMAIN,
                CLASS_DOMAIN,
                CLASS_IDENTITY_FIELD,
                fixture.class_instance.0,
                11,
            ),
            type_ref(fixture.class_instance, fixture.literal),
            type_ref(fixture.indexed, fixture.class_instance),
            type_ref(fixture.indexed, fixture.parameter),
            reference(
                CONTAINER_DOMAIN,
                TYPE_PARAM_DOMAIN,
                CONSTRAINT_OWNER_FIELD,
                0,
                fixture.parameter_id.0,
            ),
            reference(
                CONTAINER_DOMAIN,
                TYPE_DOMAIN,
                CONSTRAINT_TARGET_FIELD,
                0,
                wk.string.0,
            ),
            reference(
                CONTAINER_DOMAIN,
                TYPE_PARAM_DOMAIN,
                FROZEN_TYPE_PARAM_FIELD,
                0,
                fixture.parameter_id.0,
            ),
            reference(
                CONTAINER_DOMAIN,
                TYPE_DOMAIN,
                TEMPLATE_NAME_TYPE_FIELD,
                0,
                fixture.conditional.0,
            ),
            reference(
                CONTAINER_DOMAIN,
                TYPE_DOMAIN,
                TEMPLATE_NAME_TYPE_FIELD,
                1,
                fixture.mapped.0,
            ),
        ];
        expected_store.sort_unstable();
        assert_eq!(store_references, expected_store);

        let mut expected_interner = Vec::new();
        let mut buckets = fixture.interner.dedup.iter().collect::<Vec<_>>();
        buckets.sort_unstable_by_key(|(hash, _)| **hash);
        for (index, (_, candidates)) in buckets.into_iter().enumerate() {
            let mut candidates = candidates.iter().copied().collect::<Vec<_>>();
            candidates.sort_unstable();
            expected_interner.extend(candidates.into_iter().map(|candidate| {
                reference(
                    INTERNER_BUCKET_DOMAIN,
                    TYPE_DOMAIN,
                    BUCKET_CANDIDATE_FIELD,
                    u32::try_from(index).expect("bucket index fits u32"),
                    candidate.0,
                )
            }));
        }
        for (index, id) in [fixture.object, fixture.conditional, fixture.mapped]
            .into_iter()
            .enumerate()
        {
            expected_interner.push(reference(
                CONTAINER_DOMAIN,
                TYPE_DOMAIN,
                RESERVED_TYPE_FIELD,
                u32::try_from(index).expect("reserved index fits u32"),
                id.0,
            ));
        }
        expected_interner.extend(
            well_known_ids(wk)
                .into_iter()
                .enumerate()
                .map(|(slot, id)| {
                    reference(
                        CONTAINER_DOMAIN,
                        TYPE_DOMAIN,
                        WELL_KNOWN_TYPE_FIELD,
                        u32::try_from(slot).expect("well-known slot fits u32"),
                        id.0,
                    )
                }),
        );
        expected_interner.sort_unstable();
        assert_eq!(interner_references, expected_interner);
    }

    #[test]
    fn reference_manifest_is_canonical_and_tracks_append_only_mutation() {
        let mut interner = rich_interner();
        let before = interner.reference_records_for_test();
        assert!(before.0.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(before.1.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(interner.reference_records_for_test(), before);

        let wk = interner.well_known();
        let new_array = interner.intern_array(wk.string);
        let after = interner.reference_records_for_test();
        let added_store_reference = reference(
            TYPE_DOMAIN,
            TYPE_DOMAIN,
            TYPE_OPERAND_FIELD,
            new_array.0,
            wk.string.0,
        );
        let mut expected_store = before.0;
        expected_store.push(added_store_reference);
        expected_store.sort_unstable();
        assert_eq!(after.0, expected_store);
        assert!(after.1.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(after.1.iter().any(|record| {
            record.0 == INTERNER_BUCKET_DOMAIN
                && record.1 == TYPE_DOMAIN
                && record.2 == BUCKET_CANDIDATE_FIELD
                && record.4 == new_array.0
        }));
        assert!(after
            .0
            .iter()
            .chain(&after.1)
            .all(|record| record.0 <= 31 && record.1 <= 31 && record.2 <= 31));
    }
}

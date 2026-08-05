use super::Interner;
use crate::types::hash::{structural_hash, StructuralKey};
use crate::types::intern::{DerivationEdge, DerivedType};
use crate::types::repr::{
    DeclaredRecipeId, DeclaredRecipeNode, DeclaredType, DeclaredTypeRecipe, TupleRestType,
    TupleType, TypeFlags, TypeParamId,
};
use crate::types::store::TypeId;
use crate::types::substitute::{
    derived_free_params, substitute_derived_with_outcome, substitute_with_outcome,
    DerivedSubstitutionOutcome, SubstitutionOutcome,
};
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeclaredMaterializationError {
    InvalidRoot(TypeId),
    MissingRecipe(DeclaredRecipeId),
    CyclicRecipe(DeclaredRecipeId),
}

#[derive(Default)]
struct DeclaredRecipeTraversal {
    active: FxHashSet<DeclaredRecipeId>,
}

impl DeclaredRecipeTraversal {
    fn enter(&mut self, recipe: DeclaredRecipeId) -> Result<(), DeclaredMaterializationError> {
        if self.active.insert(recipe) {
            Ok(())
        } else {
            Err(DeclaredMaterializationError::CyclicRecipe(recipe))
        }
    }

    fn leave(&mut self, recipe: DeclaredRecipeId) {
        self.active.remove(&recipe);
    }
}

impl Interner {
    pub fn intern_declared_recipe(&mut self, node: DeclaredRecipeNode) -> DeclaredRecipeId {
        let free_params = self.derive_declared_recipe_free_params(&node);
        let recipe = DeclaredTypeRecipe { node, free_params };
        if let Some(id) = self
            .declared_recipe_local
            .get(&recipe)
            .or_else(|| self.declared_recipe_base.get(&recipe))
            .copied()
        {
            return id;
        }
        let id = self.store.push_declared_recipe(recipe.clone());
        self.declared_recipe_local.insert(recipe, id);
        id
    }

    pub(super) fn derive_declared_recipe_free_params(
        &mut self,
        node: &DeclaredRecipeNode,
    ) -> Vec<TypeParamId> {
        let mut free_params = match node {
            DeclaredRecipeNode::Type(ty) => derived_free_params(self, *ty).to_vec(),
            DeclaredRecipeNode::Array(child) | DeclaredRecipeNode::Readonly(child) => self
                .store
                .declared_recipe(*child)
                .map(|recipe| recipe.free_params.clone())
                .unwrap_or_default(),
            DeclaredRecipeNode::Tuple { elements, rest } => {
                if let Some((position, _)) = rest {
                    assert!(
                        *position <= elements.len(),
                        "declared tuple rest position is within its fixed elements"
                    );
                }
                elements
                    .iter()
                    .copied()
                    .chain(rest.iter().map(|(_, child)| *child))
                    .filter_map(|child| {
                        self.store
                            .declared_recipe(child)
                            .map(|recipe| recipe.free_params.clone())
                    })
                    .flatten()
                    .collect()
            }
            DeclaredRecipeNode::Application {
                template,
                parameters,
                arguments,
            } => {
                assert_eq!(
                    parameters.len(),
                    arguments.len(),
                    "declared application arity is canonical"
                );
                let unique: FxHashSet<_> = parameters.iter().copied().collect();
                assert_eq!(
                    unique.len(),
                    parameters.len(),
                    "declared application parameters are unique"
                );
                let mut free = derived_free_params(self, *template).to_vec();
                free.retain(|parameter| !unique.contains(parameter));
                for argument in arguments {
                    if let Some(recipe) = self.store.declared_recipe(*argument) {
                        free.extend(recipe.free_params.iter().copied());
                    }
                }
                free
            }
        };
        free_params.sort_unstable();
        free_params.dedup();
        free_params
    }

    pub fn intern_declared(
        &mut self,
        recipe: DeclaredRecipeId,
        mapper: impl IntoIterator<Item = (TypeParamId, TypeId)>,
    ) -> TypeId {
        let free_params = self
            .store
            .declared_recipe(recipe)
            .expect("declared application references an existing recipe")
            .free_params
            .clone();
        let mut mapper: Vec<_> = mapper
            .into_iter()
            .filter(|(parameter, _)| free_params.binary_search(parameter).is_ok())
            .collect();
        mapper.sort_unstable_by_key(|(parameter, _)| *parameter);
        mapper.dedup_by_key(|(parameter, _)| *parameter);
        let declared = DeclaredType { recipe, mapper };
        let key = StructuralKey::Declared {
            recipe,
            mapper: &declared.mapper,
        };
        let hash = structural_hash(&key);
        if let Some(existing) =
            self.lookup(hash, |store, id| store.declared_type(id) == Some(&declared))
        {
            return existing;
        }
        let id = self.store.push_declared(declared, TypeFlags::EMPTY);
        self.dedup.entry(hash).or_default().push(id);
        id
    }

    pub fn materialize_declared(
        &mut self,
        declared: TypeId,
    ) -> Result<SubstitutionOutcome, DeclaredMaterializationError> {
        let application = self
            .store
            .declared_type(declared)
            .cloned()
            .ok_or(DeclaredMaterializationError::InvalidRoot(declared))?;
        let mapper: FxHashMap<_, _> = application.mapper.into_iter().collect();
        self.materialize_declared_recipe(
            application.recipe,
            &mapper,
            &mut DeclaredRecipeTraversal::default(),
        )
    }

    pub fn materialize_declared_derived(
        &mut self,
        declared: TypeId,
    ) -> Result<DerivedSubstitutionOutcome, DeclaredMaterializationError> {
        let application = self
            .store
            .declared_type(declared)
            .cloned()
            .ok_or(DeclaredMaterializationError::InvalidRoot(declared))?;
        let mapper = application
            .mapper
            .into_iter()
            .map(|(parameter, ty)| (parameter, DerivedType::plain(ty)))
            .collect();
        self.materialize_declared_recipe_derived(
            application.recipe,
            &mapper,
            &mut DeclaredRecipeTraversal::default(),
        )
    }

    fn materialize_declared_recipe_derived(
        &mut self,
        recipe: DeclaredRecipeId,
        mapper: &FxHashMap<TypeParamId, DerivedType>,
        traversal: &mut DeclaredRecipeTraversal,
    ) -> Result<DerivedSubstitutionOutcome, DeclaredMaterializationError> {
        traversal.enter(recipe)?;
        let result = self.materialize_declared_recipe_derived_entered(recipe, mapper, traversal);
        traversal.leave(recipe);
        result
    }

    fn materialize_declared_recipe_derived_entered(
        &mut self,
        recipe: DeclaredRecipeId,
        mapper: &FxHashMap<TypeParamId, DerivedType>,
        traversal: &mut DeclaredRecipeTraversal,
    ) -> Result<DerivedSubstitutionOutcome, DeclaredMaterializationError> {
        let Some(node) = self
            .store
            .declared_recipe(recipe)
            .map(|recipe| recipe.node.clone())
        else {
            return Err(DeclaredMaterializationError::MissingRecipe(recipe));
        };
        match node {
            DeclaredRecipeNode::Type(ty) => Ok(DerivedSubstitutionOutcome::CycleClean(
                self.store
                    .type_param(ty)
                    .and_then(|parameter| mapper.get(&parameter.id))
                    .copied()
                    .unwrap_or_else(|| DerivedType::plain(ty)),
            )),
            DeclaredRecipeNode::Array(element) => {
                let (element, tainted) = derived_materialized_part(
                    self.materialize_declared_recipe_derived(element, mapper, traversal)?,
                );
                let result = self.intern_array(element.ty);
                let derived = self
                    .derived_wrapper(result, [(DerivationEdge::ArrayElement, element.derivation)]);
                Ok(derived_materialized_outcome(derived, tainted))
            }
            DeclaredRecipeNode::Tuple { elements, rest } => {
                let (elements, mut tainted) =
                    derived_materialized_parts(elements.into_iter().map(|element| {
                        self.materialize_declared_recipe_derived(element, mapper, traversal)
                    }))?;
                let element_types = elements.iter().map(|element| element.ty).collect();
                match rest {
                    Some((position, rest)) => {
                        let (rest, rest_tainted) = derived_materialized_part(
                            self.materialize_declared_recipe_derived(rest, mapper, traversal)?,
                        );
                        tainted |= rest_tainted;
                        let result = self.intern_tuple_type(TupleType::with_rest(
                            element_types,
                            TupleRestType::new(position, rest.ty),
                        ));
                        let children = elements
                            .iter()
                            .enumerate()
                            .map(|(index, element)| {
                                (DerivationEdge::TupleElement(index), element.derivation)
                            })
                            .chain([(DerivationEdge::TupleRest, rest.derivation)]);
                        Ok(derived_materialized_outcome(
                            self.derived_wrapper(result, children),
                            tainted,
                        ))
                    }
                    None => {
                        let result = self.intern_tuple(element_types);
                        let children = elements.iter().enumerate().map(|(index, element)| {
                            (DerivationEdge::TupleElement(index), element.derivation)
                        });
                        Ok(derived_materialized_outcome(
                            self.derived_wrapper(result, children),
                            tainted,
                        ))
                    }
                }
            }
            DeclaredRecipeNode::Readonly(operand) => {
                let (operand, tainted) = derived_materialized_part(
                    self.materialize_declared_recipe_derived(operand, mapper, traversal)?,
                );
                let result = self.intern_readonly(operand.ty);
                let derived = self.derived_wrapper(
                    result,
                    [(DerivationEdge::ReadonlyOperand, operand.derivation)],
                );
                Ok(derived_materialized_outcome(derived, tainted))
            }
            DeclaredRecipeNode::Application {
                template,
                parameters,
                arguments,
            } => {
                let (arguments, arguments_tainted) =
                    derived_materialized_parts(arguments.into_iter().map(|argument| {
                        self.materialize_declared_application_argument_derived(
                            argument, mapper, traversal,
                        )
                    }))?;
                let application_mapper = parameters
                    .into_iter()
                    .zip(arguments)
                    .collect::<FxHashMap<_, _>>();
                Ok(
                    match substitute_derived_with_outcome(self, template, &application_mapper) {
                        DerivedSubstitutionOutcome::CycleClean(result) if !arguments_tainted => {
                            DerivedSubstitutionOutcome::CycleClean(result)
                        }
                        DerivedSubstitutionOutcome::CycleClean(result)
                        | DerivedSubstitutionOutcome::CycleTainted(result) => {
                            DerivedSubstitutionOutcome::CycleTainted(DerivedType::plain(result.ty))
                        }
                    },
                )
            }
        }
    }

    fn materialize_declared_application_argument_derived(
        &mut self,
        recipe: DeclaredRecipeId,
        mapper: &FxHashMap<TypeParamId, DerivedType>,
        traversal: &mut DeclaredRecipeTraversal,
    ) -> Result<DerivedSubstitutionOutcome, DeclaredMaterializationError> {
        let node = self
            .store
            .declared_recipe(recipe)
            .map(|recipe| recipe.node.clone())
            .ok_or(DeclaredMaterializationError::MissingRecipe(recipe))?;
        if matches!(node, DeclaredRecipeNode::Application { .. }) {
            self.validate_declared_recipe(recipe, traversal)?;
            return self
                .declared_occurrence_derived(recipe, mapper)
                .map(DerivedSubstitutionOutcome::CycleClean);
        }
        self.materialize_declared_recipe_derived(recipe, mapper, traversal)
    }

    fn declared_occurrence_derived(
        &mut self,
        recipe: DeclaredRecipeId,
        mapper: &FxHashMap<TypeParamId, DerivedType>,
    ) -> Result<DerivedType, DeclaredMaterializationError> {
        let node = self
            .store
            .declared_recipe(recipe)
            .map(|recipe| recipe.node.clone())
            .ok_or(DeclaredMaterializationError::MissingRecipe(recipe))?;
        let result = self.intern_declared(
            recipe,
            mapper
                .iter()
                .map(|(&parameter, derived)| (parameter, derived.ty)),
        );
        let declared = self
            .store
            .declared_type(result)
            .cloned()
            .ok_or(DeclaredMaterializationError::InvalidRoot(result))?;
        let mut children = declared
            .mapper
            .iter()
            .enumerate()
            .filter_map(|(index, (parameter, _))| {
                mapper
                    .get(parameter)
                    .and_then(|derived| derived.derivation)
                    .map(|derivation| (DerivationEdge::DeclaredMapper(index), derivation))
            })
            .collect::<Vec<_>>();
        let identity = if let DeclaredRecipeNode::Application {
            template,
            arguments,
            ..
        } = node
        {
            for (index, argument) in arguments.into_iter().enumerate() {
                let child = self.declared_occurrence_derived(argument, mapper)?;
                if let Some(derivation) = child.derivation {
                    children.push((DerivationEdge::DeclaredArgument(index), derivation));
                }
            }
            template
        } else {
            result
        };
        let derivation = self.intern_derivation(result, identity, children);
        Ok(DerivedType {
            ty: result,
            derivation: Some(derivation),
        })
    }

    fn derived_wrapper(
        &mut self,
        result: TypeId,
        children: impl IntoIterator<Item = (DerivationEdge, Option<super::DerivationId>)>,
    ) -> DerivedType {
        let children = children
            .into_iter()
            .filter_map(|(edge, derivation)| derivation.map(|derivation| (edge, derivation)))
            .collect::<Vec<_>>();
        if children.is_empty() {
            return DerivedType::plain(result);
        }
        let derivation = self.intern_derivation(result, result, children);
        DerivedType {
            ty: result,
            derivation: Some(derivation),
        }
    }

    fn materialize_declared_recipe(
        &mut self,
        recipe: DeclaredRecipeId,
        mapper: &FxHashMap<TypeParamId, TypeId>,
        traversal: &mut DeclaredRecipeTraversal,
    ) -> Result<SubstitutionOutcome, DeclaredMaterializationError> {
        traversal.enter(recipe)?;
        let result = self.materialize_declared_recipe_entered(recipe, mapper, traversal);
        traversal.leave(recipe);
        result
    }

    fn materialize_declared_recipe_entered(
        &mut self,
        recipe: DeclaredRecipeId,
        mapper: &FxHashMap<TypeParamId, TypeId>,
        traversal: &mut DeclaredRecipeTraversal,
    ) -> Result<SubstitutionOutcome, DeclaredMaterializationError> {
        let node = self
            .store
            .declared_recipe(recipe)
            .map(|recipe| recipe.node.clone())
            .ok_or(DeclaredMaterializationError::MissingRecipe(recipe))?;
        match node {
            DeclaredRecipeNode::Type(ty) => Ok(SubstitutionOutcome::CycleClean(
                self.store
                    .type_param(ty)
                    .and_then(|parameter| mapper.get(&parameter.id))
                    .copied()
                    .unwrap_or(ty),
            )),
            DeclaredRecipeNode::Array(element) => {
                let (element, tainted) = materialized_parts([
                    self.materialize_declared_recipe(element, mapper, traversal)
                ])?;
                Ok(materialized_outcome(self.intern_array(element[0]), tainted))
            }
            DeclaredRecipeNode::Tuple { elements, rest } => {
                let (elements, mut tainted) =
                    materialized_parts(elements.into_iter().map(|element| {
                        self.materialize_declared_recipe(element, mapper, traversal)
                    }))?;
                match rest {
                    Some((position, rest)) => {
                        let rest = self.materialize_declared_recipe(rest, mapper, traversal)?;
                        let (rest, rest_tainted) = materialized_part(rest);
                        tainted |= rest_tainted;
                        Ok(materialized_outcome(
                            self.intern_tuple_type(TupleType::with_rest(
                                elements,
                                TupleRestType::new(position, rest),
                            )),
                            tainted,
                        ))
                    }
                    None => Ok(materialized_outcome(self.intern_tuple(elements), tainted)),
                }
            }
            DeclaredRecipeNode::Readonly(operand) => {
                let operand = self.materialize_declared_recipe(operand, mapper, traversal)?;
                let (operand, tainted) = materialized_part(operand);
                Ok(materialized_outcome(self.intern_readonly(operand), tainted))
            }
            DeclaredRecipeNode::Application {
                template,
                parameters,
                arguments,
            } => {
                let (arguments, arguments_tainted) =
                    materialized_parts(arguments.into_iter().map(|argument| {
                        self.materialize_declared_application_argument(argument, mapper, traversal)
                    }))?;
                let application_mapper = parameters
                    .into_iter()
                    .zip(arguments)
                    .collect::<FxHashMap<_, _>>();
                Ok(
                    match substitute_with_outcome(self, template, &application_mapper) {
                        SubstitutionOutcome::CycleClean(result) if !arguments_tainted => {
                            SubstitutionOutcome::CycleClean(result)
                        }
                        SubstitutionOutcome::CycleClean(result)
                        | SubstitutionOutcome::CycleTainted(result) => {
                            SubstitutionOutcome::CycleTainted(result)
                        }
                    },
                )
            }
        }
    }

    fn materialize_declared_application_argument(
        &mut self,
        recipe: DeclaredRecipeId,
        mapper: &FxHashMap<TypeParamId, TypeId>,
        traversal: &mut DeclaredRecipeTraversal,
    ) -> Result<SubstitutionOutcome, DeclaredMaterializationError> {
        let node = self
            .store
            .declared_recipe(recipe)
            .map(|recipe| recipe.node.clone())
            .ok_or(DeclaredMaterializationError::MissingRecipe(recipe))?;
        if matches!(node, DeclaredRecipeNode::Application { .. }) {
            self.validate_declared_recipe(recipe, traversal)?;
            return Ok(SubstitutionOutcome::CycleClean(self.intern_declared(
                recipe,
                mapper.iter().map(|(&parameter, &ty)| (parameter, ty)),
            )));
        }
        self.materialize_declared_recipe(recipe, mapper, traversal)
    }

    fn validate_declared_recipe(
        &self,
        recipe: DeclaredRecipeId,
        traversal: &mut DeclaredRecipeTraversal,
    ) -> Result<(), DeclaredMaterializationError> {
        traversal.enter(recipe)?;
        let result = self.validate_declared_recipe_entered(recipe, traversal);
        traversal.leave(recipe);
        result
    }

    fn validate_declared_recipe_entered(
        &self,
        recipe: DeclaredRecipeId,
        traversal: &mut DeclaredRecipeTraversal,
    ) -> Result<(), DeclaredMaterializationError> {
        let node = self
            .store
            .declared_recipe(recipe)
            .map(|recipe| recipe.node.clone())
            .ok_or(DeclaredMaterializationError::MissingRecipe(recipe))?;
        match node {
            DeclaredRecipeNode::Type(_) => Ok(()),
            DeclaredRecipeNode::Array(child) | DeclaredRecipeNode::Readonly(child) => {
                self.validate_declared_recipe(child, traversal)
            }
            DeclaredRecipeNode::Tuple { elements, rest } => {
                for child in elements
                    .into_iter()
                    .chain(rest.into_iter().map(|(_, child)| child))
                {
                    self.validate_declared_recipe(child, traversal)?;
                }
                Ok(())
            }
            DeclaredRecipeNode::Application { arguments, .. } => {
                for argument in arguments {
                    self.validate_declared_recipe(argument, traversal)?;
                }
                Ok(())
            }
        }
    }
}

fn materialized_part(outcome: SubstitutionOutcome) -> (TypeId, bool) {
    match outcome {
        SubstitutionOutcome::CycleClean(result) => (result, false),
        SubstitutionOutcome::CycleTainted(result) => (result, true),
    }
}

fn materialized_parts(
    outcomes: impl IntoIterator<Item = Result<SubstitutionOutcome, DeclaredMaterializationError>>,
) -> Result<(Vec<TypeId>, bool), DeclaredMaterializationError> {
    let mut tainted = false;
    let mut results = Vec::new();
    for outcome in outcomes {
        let (result, outcome_tainted) = materialized_part(outcome?);
        tainted |= outcome_tainted;
        results.push(result);
    }
    Ok((results, tainted))
}

fn materialized_outcome(result: TypeId, tainted: bool) -> SubstitutionOutcome {
    if tainted {
        SubstitutionOutcome::CycleTainted(result)
    } else {
        SubstitutionOutcome::CycleClean(result)
    }
}

fn derived_materialized_part(outcome: DerivedSubstitutionOutcome) -> (DerivedType, bool) {
    match outcome {
        DerivedSubstitutionOutcome::CycleClean(result) => (result, false),
        DerivedSubstitutionOutcome::CycleTainted(result) => (result, true),
    }
}

fn derived_materialized_parts(
    outcomes: impl IntoIterator<Item = Result<DerivedSubstitutionOutcome, DeclaredMaterializationError>>,
) -> Result<(Vec<DerivedType>, bool), DeclaredMaterializationError> {
    let mut tainted = false;
    let mut results = Vec::new();
    for outcome in outcomes {
        let outcome = outcome?;
        let (result, outcome_tainted) = derived_materialized_part(outcome);
        tainted |= outcome_tainted;
        results.push(result);
    }
    Ok((results, tainted))
}

fn derived_materialized_outcome(result: DerivedType, tainted: bool) -> DerivedSubstitutionOutcome {
    if tainted {
        DerivedSubstitutionOutcome::CycleTainted(DerivedType::plain(result.ty))
    } else {
        DerivedSubstitutionOutcome::CycleClean(result)
    }
}

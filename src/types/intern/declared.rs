use super::Interner;
use crate::types::hash::{structural_hash, StructuralKey};
use crate::types::repr::{
    DeclaredRecipeId, DeclaredRecipeNode, DeclaredType, DeclaredTypeRecipe, TupleRestType,
    TupleType, TypeFlags, TypeParamId,
};
use crate::types::store::TypeId;
use crate::types::substitute::{derived_free_params, substitute_with_outcome, SubstitutionOutcome};
use rustc_hash::{FxHashMap, FxHashSet};

impl Interner {
    pub(crate) fn intern_declared_recipe(&mut self, node: DeclaredRecipeNode) -> DeclaredRecipeId {
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
                .expect("declared recipe child exists")
                .free_params
                .clone(),
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
                    .flat_map(|child| {
                        self.store
                            .declared_recipe(child)
                            .expect("declared recipe child exists")
                            .free_params
                            .clone()
                    })
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
                    free.extend(
                        self.store
                            .declared_recipe(*argument)
                            .expect("declared recipe child exists")
                            .free_params
                            .iter()
                            .copied(),
                    );
                }
                free
            }
        };
        free_params.sort_unstable();
        free_params.dedup();
        free_params
    }

    pub(crate) fn intern_declared(
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

    pub(crate) fn materialize_declared(&mut self, declared: TypeId) -> Option<SubstitutionOutcome> {
        let application = self.store.declared_type(declared)?.clone();
        let mapper: FxHashMap<_, _> = application.mapper.into_iter().collect();
        Some(self.materialize_declared_recipe(application.recipe, &mapper))
    }

    fn materialize_declared_recipe(
        &mut self,
        recipe: DeclaredRecipeId,
        mapper: &FxHashMap<TypeParamId, TypeId>,
    ) -> SubstitutionOutcome {
        let node = self
            .store
            .declared_recipe(recipe)
            .expect("declared recipe child exists")
            .node
            .clone();
        match node {
            DeclaredRecipeNode::Type(ty) => SubstitutionOutcome::CycleClean(
                self.store
                    .type_param(ty)
                    .and_then(|parameter| mapper.get(&parameter.id))
                    .copied()
                    .unwrap_or(ty),
            ),
            DeclaredRecipeNode::Array(element) => {
                let (element, tainted) =
                    materialized_parts([self.materialize_declared_recipe(element, mapper)]);
                materialized_outcome(self.intern_array(element[0]), tainted)
            }
            DeclaredRecipeNode::Tuple { elements, rest } => {
                let (elements, mut tainted) = materialized_parts(
                    elements
                        .into_iter()
                        .map(|element| self.materialize_declared_recipe(element, mapper)),
                );
                match rest {
                    Some((position, rest)) => {
                        let rest = self.materialize_declared_recipe(rest, mapper);
                        let (rest, rest_tainted) = materialized_part(rest);
                        tainted |= rest_tainted;
                        materialized_outcome(
                            self.intern_tuple_type(TupleType::with_rest(
                                elements,
                                TupleRestType::new(position, rest),
                            )),
                            tainted,
                        )
                    }
                    None => materialized_outcome(self.intern_tuple(elements), tainted),
                }
            }
            DeclaredRecipeNode::Readonly(operand) => {
                let (operand, tainted) =
                    materialized_part(self.materialize_declared_recipe(operand, mapper));
                materialized_outcome(self.intern_readonly(operand), tainted)
            }
            DeclaredRecipeNode::Application {
                template,
                parameters,
                arguments,
            } => {
                let (arguments, arguments_tainted) = materialized_parts(
                    arguments
                        .into_iter()
                        .map(|argument| self.materialize_declared_recipe(argument, mapper)),
                );
                let application_mapper = parameters
                    .into_iter()
                    .zip(arguments)
                    .collect::<FxHashMap<_, _>>();
                match substitute_with_outcome(self, template, &application_mapper) {
                    SubstitutionOutcome::CycleClean(result) if !arguments_tainted => {
                        SubstitutionOutcome::CycleClean(result)
                    }
                    SubstitutionOutcome::CycleClean(result)
                    | SubstitutionOutcome::CycleTainted(result) => {
                        SubstitutionOutcome::CycleTainted(result)
                    }
                }
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
    outcomes: impl IntoIterator<Item = SubstitutionOutcome>,
) -> (Vec<TypeId>, bool) {
    let mut tainted = false;
    let results = outcomes
        .into_iter()
        .map(|outcome| {
            let (result, outcome_tainted) = materialized_part(outcome);
            tainted |= outcome_tainted;
            result
        })
        .collect();
    (results, tainted)
}

fn materialized_outcome(result: TypeId, tainted: bool) -> SubstitutionOutcome {
    if tainted {
        SubstitutionOutcome::CycleTainted(result)
    } else {
        SubstitutionOutcome::CycleClean(result)
    }
}

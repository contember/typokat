use super::*;
use crate::check::checker::context::TypeDecl;
use crate::check::checker::library_identities::NativeArrayAlias;
use crate::check::checker::type_groups::{PublishedTypeGroupSurface, PublishedTypeGroupTerminal};
use crate::types::repr::{DeclaredRecipeId, DeclaredRecipeNode, TypeParamId};
use oxc_ast::ast::{TSTupleElement, TSTypeName};

enum PlannedAnnotation {
    Existing(TypeId),
    Deferred(PlannedRecipe),
    Fallback,
}

struct PlannedRoot<'name> {
    annotation: PlannedAnnotation,
    dependencies: Vec<PlannedTypeDependency<'name>>,
}

struct PlannedTypeDependency<'name> {
    scope: ScopeId,
    name: &'name str,
}

struct PlannedRecipe {
    node: PlannedRecipeNode,
    contains_tuple: bool,
}

enum PlannedRecipeNode {
    Type(TypeId),
    Array(Box<PlannedRecipe>),
    Tuple(Vec<PlannedRecipe>),
    Readonly(Box<PlannedRecipe>),
    Application {
        template: TypeId,
        parameters: Vec<TypeParamId>,
        arguments: Vec<PlannedRecipe>,
    },
}

impl PlannedRecipe {
    fn ty(ty: TypeId) -> Self {
        Self {
            node: PlannedRecipeNode::Type(ty),
            contains_tuple: false,
        }
    }

    fn array(element: PlannedRecipe) -> Self {
        Self {
            contains_tuple: element.contains_tuple,
            node: PlannedRecipeNode::Array(Box::new(element)),
        }
    }

    fn tuple(elements: Vec<PlannedRecipe>) -> Self {
        Self {
            contains_tuple: true,
            node: PlannedRecipeNode::Tuple(elements),
        }
    }

    fn readonly(operand: PlannedRecipe) -> Self {
        Self {
            contains_tuple: operand.contains_tuple,
            node: PlannedRecipeNode::Readonly(Box::new(operand)),
        }
    }

    fn application(
        template: TypeId,
        parameters: Vec<TypeParamId>,
        arguments: Vec<PlannedRecipe>,
    ) -> Self {
        Self {
            contains_tuple: arguments.iter().any(|argument| argument.contains_tuple),
            node: PlannedRecipeNode::Application {
                template,
                parameters,
                arguments,
            },
        }
    }
}

impl<'a, 'ast, Ticket: Copy + PartialEq> Pass<'a, 'ast, Ticket> {
    pub(in crate::check::checker) fn try_plan_declared_interface_property_annotation<
        'node,
        'syntax,
    >(
        &mut self,
        scope: ScopeId,
        annotation: &'node TSType<'syntax>,
    ) -> Option<TypeId> {
        let plan = self.plan_declared_annotation(scope, annotation, true);
        self.commit_declared_plan(plan)
    }

    pub(in crate::check::checker) fn try_plan_declared_annotation<'node, 'syntax>(
        &mut self,
        scope: ScopeId,
        annotation: &'node TSType<'syntax>,
    ) -> Option<TypeId> {
        let plan = self.plan_declared_annotation(scope, annotation, false);
        self.commit_declared_plan(plan)
    }

    fn plan_declared_annotation<'node, 'syntax>(
        &self,
        scope: ScopeId,
        annotation: &'node TSType<'syntax>,
        reject_tuple: bool,
    ) -> PlannedRoot<'node> {
        let mut dependencies = Vec::new();
        let Some(recipe) = self.plan_declared_recipe(
            scope,
            annotation,
            self.annotation_depth + 1,
            &mut dependencies,
        ) else {
            return PlannedRoot {
                annotation: PlannedAnnotation::Fallback,
                dependencies: Vec::new(),
            };
        };
        if reject_tuple && recipe.contains_tuple {
            return PlannedRoot {
                annotation: PlannedAnnotation::Fallback,
                dependencies: Vec::new(),
            };
        }
        let annotation = match recipe.node {
            PlannedRecipeNode::Type(ty) => PlannedAnnotation::Existing(ty),
            _ => PlannedAnnotation::Deferred(recipe),
        };
        PlannedRoot {
            annotation,
            dependencies,
        }
    }

    fn commit_declared_plan(&mut self, plan: PlannedRoot<'_>) -> Option<TypeId> {
        let ty = match plan.annotation {
            PlannedAnnotation::Existing(ty) => ty,
            PlannedAnnotation::Deferred(recipe) => {
                let recipe = self.commit_declared_recipe(recipe);
                self.interner.intern_declared(recipe, [])
            }
            PlannedAnnotation::Fallback => return None,
        };
        for dependency in plan.dependencies {
            // Replays the demand the planner read untraced, so the replay index records it.
            self.type_decl_id_replay(dependency.scope, dependency.name);
        }
        Some(ty)
    }

    fn commit_declared_recipe(&mut self, recipe: PlannedRecipe) -> DeclaredRecipeId {
        let node = match recipe.node {
            PlannedRecipeNode::Type(ty) => DeclaredRecipeNode::Type(ty),
            PlannedRecipeNode::Array(element) => {
                DeclaredRecipeNode::Array(self.commit_declared_recipe(*element))
            }
            PlannedRecipeNode::Tuple(elements) => DeclaredRecipeNode::Tuple {
                elements: elements
                    .into_iter()
                    .map(|element| self.commit_declared_recipe(element))
                    .collect(),
                rest: None,
            },
            PlannedRecipeNode::Readonly(operand) => {
                DeclaredRecipeNode::Readonly(self.commit_declared_recipe(*operand))
            }
            PlannedRecipeNode::Application {
                template,
                parameters,
                arguments,
            } => DeclaredRecipeNode::Application {
                template,
                parameters,
                arguments: arguments
                    .into_iter()
                    .map(|argument| self.commit_declared_recipe(argument))
                    .collect(),
            },
        };
        self.interner.intern_declared_recipe(node)
    }

    fn plan_declared_recipe<'node, 'syntax>(
        &self,
        scope: ScopeId,
        annotation: &'node TSType<'syntax>,
        depth: u32,
        dependencies: &mut Vec<PlannedTypeDependency<'node>>,
    ) -> Option<PlannedRecipe> {
        if depth > MAX_ANNOTATION_DEPTH {
            return None;
        }
        let wk = self.interner.well_known();
        let leaf = match annotation {
            TSType::TSAnyKeyword(_) => Some(wk.any),
            TSType::TSUnknownKeyword(_) => Some(wk.unknown),
            TSType::TSNeverKeyword(_) => Some(wk.never),
            TSType::TSVoidKeyword(_) => Some(wk.void),
            TSType::TSNullKeyword(_) => Some(wk.null),
            TSType::TSUndefinedKeyword(_) => Some(wk.undefined),
            TSType::TSBooleanKeyword(_) => Some(wk.boolean),
            TSType::TSNumberKeyword(_) => Some(wk.number),
            TSType::TSStringKeyword(_) => Some(wk.string),
            _ => None,
        };
        if let Some(ty) = leaf {
            return Some(PlannedRecipe::ty(ty));
        }
        match annotation {
            TSType::TSParenthesizedType(parenthesized) => self.plan_declared_recipe(
                scope,
                &parenthesized.type_annotation,
                depth + 1,
                dependencies,
            ),
            TSType::TSArrayType(array) => {
                let element =
                    self.plan_declared_recipe(scope, &array.element_type, depth + 1, dependencies)?;
                Some(PlannedRecipe::array(element))
            }
            TSType::TSTupleType(tuple) => {
                let mut elements = Vec::with_capacity(tuple.element_types.len());
                for element in &tuple.element_types {
                    elements.push(self.plan_declared_tuple_element(
                        scope,
                        element,
                        depth + 1,
                        dependencies,
                    )?);
                }
                Some(PlannedRecipe::tuple(elements))
            }
            TSType::TSTypeOperatorType(operator)
                if operator.operator == TSTypeOperatorOperator::Readonly =>
            {
                if !matches!(
                    &operator.type_annotation,
                    TSType::TSArrayType(_) | TSType::TSTupleType(_)
                ) {
                    return None;
                }
                let operand = self.plan_declared_recipe(
                    scope,
                    &operator.type_annotation,
                    depth,
                    dependencies,
                )?;
                Some(PlannedRecipe::readonly(operand))
            }
            TSType::TSTypeReference(reference) => self.plan_declared_reference(
                scope,
                &reference.type_name,
                reference.type_arguments.as_deref(),
                depth,
                dependencies,
            ),
            _ => None,
        }
    }

    fn plan_declared_tuple_element<'node, 'syntax>(
        &self,
        scope: ScopeId,
        element: &'node TSTupleElement<'syntax>,
        depth: u32,
        dependencies: &mut Vec<PlannedTypeDependency<'node>>,
    ) -> Option<PlannedRecipe> {
        match element {
            TSTupleElement::TSNamedTupleMember(named) if !named.optional => {
                self.plan_declared_tuple_element(scope, &named.element_type, depth, dependencies)
            }
            TSTupleElement::TSOptionalType(_) | TSTupleElement::TSRestType(_) => None,
            _ => element
                .as_ts_type()
                .and_then(|element| self.plan_declared_recipe(scope, element, depth, dependencies)),
        }
    }

    fn plan_declared_reference<'node, 'syntax>(
        &self,
        scope: ScopeId,
        name: &'node TSTypeName<'syntax>,
        arguments: Option<&'node oxc_ast::ast::TSTypeParameterInstantiation<'syntax>>,
        depth: u32,
        dependencies: &mut Vec<PlannedTypeDependency<'node>>,
    ) -> Option<PlannedRecipe> {
        let TSTypeName::IdentifierReference(identifier) = name else {
            return None;
        };
        let name = identifier.name.as_str();
        if arguments.is_none() {
            if let Some(ty) = self.lookup_type_param(name) {
                self.interner.store().type_param(ty)?;
                return Some(PlannedRecipe::ty(ty));
            }
        }
        let group = self.peek_type_decl_id_untraced(scope, name)?;
        if !self.type_group_construction_is_frozen(group) {
            return None;
        }
        // `Array<E>` / `ReadonlyArray<E>` name the native array types, not the library
        // interface body that carries their member surface (see `library_identities`).
        if let Some(alias) = self.native_array_groups().alias_of(group) {
            let [argument] = arguments.map(|arguments| arguments.params.as_slice())? else {
                return None;
            };
            let element = self.plan_declared_recipe(scope, argument, depth + 1, dependencies)?;
            let array = PlannedRecipe::array(element);
            dependencies.push(PlannedTypeDependency { scope, name });
            return Some(match alias {
                NativeArrayAlias::Array => array,
                NativeArrayAlias::ReadonlyArray => PlannedRecipe::readonly(array),
            });
        }
        let (template, parameters, published_template) = match self.type_decls.get(group.index()) {
            Some(TypeDecl::Interface {
                reserved,
                recovery_params,
                ..
            }) => (*reserved, recovery_params.clone(), false),
            _ => match self
                .type_environment
                .resolution_environment()
                .groups()
                .get(group)?
            {
                PublishedTypeGroupTerminal::Ready(group) => match group.surface {
                    PublishedTypeGroupSurface::Template(template) => {
                        (template, group.parameters.clone(), true)
                    }
                    PublishedTypeGroupSurface::Class(_) => return None,
                },
                PublishedTypeGroupTerminal::Unavailable(_) => return None,
            },
        };
        if parameters.iter().any(|parameter| {
            self.interner
                .store()
                .type_param_constraint(*parameter)
                .is_some()
        }) {
            return None;
        }
        let recipe = match arguments {
            Some(arguments) if arguments.params.is_empty() && parameters.is_empty() => {
                PlannedRecipe::ty(template)
            }
            Some(arguments) if arguments.params.len() == parameters.len() => {
                if published_template && self.interner.store().tag(template) != TypeTag::Object {
                    return None;
                }
                let mut planned = Vec::with_capacity(arguments.params.len());
                for argument in &arguments.params {
                    planned.push(self.plan_declared_recipe(
                        scope,
                        argument,
                        depth + 1,
                        dependencies,
                    )?);
                }
                PlannedRecipe::application(template, parameters, planned)
            }
            None if parameters.is_empty() => PlannedRecipe::ty(template),
            _ => return None,
        };
        dependencies.push(PlannedTypeDependency { scope, name });
        Some(recipe)
    }
}

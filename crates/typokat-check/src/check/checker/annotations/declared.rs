use super::*;
use crate::binder::declaration::TypeFragmentKind;
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
    captures_selected_binder: bool,
}

struct PlannedApplicationArgument {
    recipe: PlannedRecipe,
    span: Span,
}

struct PlannedConstraintObligation {
    parameters: Vec<TypeParamId>,
    arguments: Vec<(TypeId, Span)>,
}

enum PlannedRecipeNode {
    Type(TypeId),
    Literal(LiteralValue),
    Array(Box<PlannedRecipe>),
    Tuple(Vec<PlannedRecipe>),
    Readonly(Box<PlannedRecipe>),
    Application {
        template: TypeId,
        parameters: Vec<TypeParamId>,
        arguments: Vec<PlannedApplicationArgument>,
    },
}

impl PlannedRecipe {
    fn ty(ty: TypeId, captures_selected_binder: bool) -> Self {
        Self {
            node: PlannedRecipeNode::Type(ty),
            contains_tuple: false,
            captures_selected_binder,
        }
    }

    fn literal(value: LiteralValue) -> Self {
        Self {
            node: PlannedRecipeNode::Literal(value),
            contains_tuple: false,
            captures_selected_binder: false,
        }
    }

    fn array(element: PlannedRecipe) -> Self {
        Self {
            contains_tuple: element.contains_tuple,
            captures_selected_binder: element.captures_selected_binder,
            node: PlannedRecipeNode::Array(Box::new(element)),
        }
    }

    fn tuple(elements: Vec<PlannedRecipe>) -> Self {
        Self {
            contains_tuple: true,
            captures_selected_binder: elements
                .iter()
                .any(|element| element.captures_selected_binder),
            node: PlannedRecipeNode::Tuple(elements),
        }
    }

    fn readonly(operand: PlannedRecipe) -> Self {
        Self {
            contains_tuple: operand.contains_tuple,
            captures_selected_binder: operand.captures_selected_binder,
            node: PlannedRecipeNode::Readonly(Box::new(operand)),
        }
    }

    fn application(
        template: TypeId,
        parameters: Vec<TypeParamId>,
        arguments: Vec<PlannedApplicationArgument>,
    ) -> Self {
        let contains_tuple = arguments
            .iter()
            .any(|argument| argument.recipe.contains_tuple);
        let captures_selected_binder = arguments
            .iter()
            .any(|argument| argument.recipe.captures_selected_binder);
        Self {
            contains_tuple,
            captures_selected_binder,
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
        let plan = self.plan_declared_annotation(scope, annotation, true, None);
        self.commit_declared_plan(plan)
    }

    pub(in crate::check::checker) fn try_plan_declared_annotation<'node, 'syntax>(
        &mut self,
        scope: ScopeId,
        annotation: &'node TSType<'syntax>,
    ) -> Option<TypeId> {
        let plan = self.plan_declared_annotation(scope, annotation, false, None);
        self.commit_declared_plan(plan)
    }

    fn try_plan_declared_application_annotation<'node, 'syntax>(
        &mut self,
        scope: ScopeId,
        annotation: &'node TSType<'syntax>,
    ) -> Option<TypeId> {
        let plan = self.plan_declared_annotation(scope, annotation, false, None);
        if !matches!(
            &plan.annotation,
            PlannedAnnotation::Deferred(PlannedRecipe {
                node: PlannedRecipeNode::Application { .. },
                ..
            })
        ) {
            return None;
        }
        self.commit_declared_plan(plan)
    }

    pub(in crate::check::checker) fn lower_declared_annotation_or_fallback(
        &mut self,
        scope: ScopeId,
        annotation: &TSType<'_>,
        with_indirection: bool,
    ) -> Option<TypeId> {
        if let Some(ty) = self.try_plan_declared_application_annotation(scope, annotation) {
            return Some(ty);
        }
        if with_indirection {
            self.with_indirection(|pass| pass.lower_annotation(scope, annotation))
        } else {
            self.lower_annotation(scope, annotation)
        }
    }

    /// Preserve a callable-edge application only when its syntax captures a binder
    /// that is actually visible at this position. Other annotations stay on their
    /// established lowering route.
    pub(in crate::check::checker) fn try_plan_declared_callable_annotation<'node, 'syntax>(
        &mut self,
        scope: ScopeId,
        annotation: &'node TSType<'syntax>,
    ) -> Option<TypeId> {
        let selected = self.visible_callable_type_params();
        let plan = self.plan_declared_annotation(scope, annotation, false, Some(&selected));
        let eligible = match &plan.annotation {
            PlannedAnnotation::Deferred(
                recipe @ PlannedRecipe {
                    node: PlannedRecipeNode::Application { .. },
                    ..
                },
            ) => recipe.captures_selected_binder || self.planned_recipe_contains_constraint(recipe),
            _ => false,
        };
        if !eligible {
            return None;
        }
        self.commit_declared_plan(plan)
    }

    fn planned_recipe_contains_constraint(&self, recipe: &PlannedRecipe) -> bool {
        match &recipe.node {
            PlannedRecipeNode::Type(_) | PlannedRecipeNode::Literal(_) => false,
            PlannedRecipeNode::Array(child) | PlannedRecipeNode::Readonly(child) => {
                self.planned_recipe_contains_constraint(child)
            }
            PlannedRecipeNode::Tuple(elements) => elements
                .iter()
                .any(|element| self.planned_recipe_contains_constraint(element)),
            PlannedRecipeNode::Application {
                parameters,
                arguments,
                ..
            } => {
                parameters.iter().any(|parameter| {
                    self.interner
                        .store()
                        .type_param_constraint(*parameter)
                        .is_some()
                }) || arguments
                    .iter()
                    .any(|argument| self.planned_recipe_contains_constraint(&argument.recipe))
            }
        }
    }

    /// Select by declaration identity after lexical shadowing and static-class barriers.
    fn visible_callable_type_params(&self) -> FxHashSet<TypeParamId> {
        let mut seen_names = FxHashSet::default();
        let mut visible = FxHashSet::default();
        for frame in self.type_param_scopes.iter().rev() {
            for (name, ty) in frame {
                if !seen_names.insert(name.as_str()) {
                    continue;
                }
                let Some(parameter) = self.interner.store().type_param(*ty) else {
                    continue;
                };
                if self
                    .static_class_type_param_barriers
                    .iter()
                    .rev()
                    .any(|barrier| barrier.contains(&parameter.id))
                {
                    continue;
                }
                visible.insert(parameter.id);
            }
        }
        visible
    }

    pub(in crate::check::checker) fn lower_callable_annotation(
        &mut self,
        scope: ScopeId,
        annotation: &TSType<'_>,
        with_indirection: bool,
    ) -> Option<TypeId> {
        if let Some(ty) = self.try_plan_declared_callable_annotation(scope, annotation) {
            return Some(ty);
        }
        if with_indirection {
            self.with_indirection(|pass| pass.lower_annotation(scope, annotation))
        } else {
            self.lower_annotation(scope, annotation)
        }
    }

    fn plan_declared_annotation<'node, 'syntax>(
        &self,
        scope: ScopeId,
        annotation: &'node TSType<'syntax>,
        reject_tuple: bool,
        selected_binders: Option<&FxHashSet<TypeParamId>>,
    ) -> PlannedRoot<'node> {
        let mut dependencies = Vec::new();
        let Some(recipe) = self.plan_declared_recipe(
            scope,
            annotation,
            self.annotation_depth + 1,
            &mut dependencies,
            selected_binders,
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
            PlannedRecipeNode::Literal(_) => PlannedAnnotation::Fallback,
            _ => PlannedAnnotation::Deferred(recipe),
        };
        PlannedRoot {
            annotation,
            dependencies,
        }
    }

    fn commit_declared_plan(&mut self, plan: PlannedRoot<'_>) -> Option<TypeId> {
        let mut constraint_obligations = Vec::new();
        let ty = match plan.annotation {
            PlannedAnnotation::Existing(ty) => ty,
            PlannedAnnotation::Deferred(recipe) => {
                let recipe = self.commit_declared_recipe(recipe, &mut constraint_obligations);
                self.interner.intern_declared(recipe, [])
            }
            PlannedAnnotation::Fallback => return None,
        };
        for obligation in constraint_obligations.into_iter().flatten() {
            let substitutions = obligation
                .parameters
                .iter()
                .copied()
                .zip(obligation.arguments.iter().map(|(ty, _)| *ty))
                .collect();
            self.check_type_argument_constraints(
                &obligation.parameters,
                &obligation.arguments,
                &substitutions,
            );
        }
        for dependency in plan.dependencies {
            // Replays the demand the planner read untraced, so the replay index records it.
            self.type_decl_id_replay(dependency.scope, dependency.name);
        }
        Some(ty)
    }

    fn commit_declared_recipe(
        &mut self,
        recipe: PlannedRecipe,
        constraint_obligations: &mut Vec<Option<PlannedConstraintObligation>>,
    ) -> DeclaredRecipeId {
        let node = match recipe.node {
            PlannedRecipeNode::Type(ty) => DeclaredRecipeNode::Type(ty),
            PlannedRecipeNode::Literal(value) => {
                DeclaredRecipeNode::Type(self.interner.intern_literal(value))
            }
            PlannedRecipeNode::Array(element) => DeclaredRecipeNode::Array(
                self.commit_declared_recipe(*element, constraint_obligations),
            ),
            PlannedRecipeNode::Tuple(elements) => DeclaredRecipeNode::Tuple {
                elements: elements
                    .into_iter()
                    .map(|element| self.commit_declared_recipe(element, constraint_obligations))
                    .collect(),
                rest: None,
            },
            PlannedRecipeNode::Readonly(operand) => DeclaredRecipeNode::Readonly(
                self.commit_declared_recipe(*operand, constraint_obligations),
            ),
            PlannedRecipeNode::Application {
                template,
                parameters,
                arguments,
            } => {
                let obligation_index = parameters
                    .iter()
                    .any(|parameter| {
                        self.interner
                            .store()
                            .type_param_constraint(*parameter)
                            .is_some()
                    })
                    .then(|| {
                        let index = constraint_obligations.len();
                        constraint_obligations.push(None);
                        index
                    });
                let arguments = arguments
                    .into_iter()
                    .map(|argument| {
                        let recipe =
                            self.commit_declared_recipe(argument.recipe, constraint_obligations);
                        let ty = self.interner.intern_declared(recipe, []);
                        (recipe, ty, argument.span)
                    })
                    .collect::<Vec<_>>();
                if let Some(index) = obligation_index {
                    constraint_obligations[index] = Some(PlannedConstraintObligation {
                        parameters: parameters.clone(),
                        arguments: arguments.iter().map(|(_, ty, span)| (*ty, *span)).collect(),
                    });
                }
                DeclaredRecipeNode::Application {
                    template,
                    parameters,
                    arguments: arguments.into_iter().map(|(recipe, _, _)| recipe).collect(),
                }
            }
        };
        self.interner.intern_declared_recipe(node)
    }

    fn plan_declared_recipe<'node, 'syntax>(
        &self,
        scope: ScopeId,
        annotation: &'node TSType<'syntax>,
        depth: u32,
        dependencies: &mut Vec<PlannedTypeDependency<'node>>,
        selected_binders: Option<&FxHashSet<TypeParamId>>,
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
            return Some(PlannedRecipe::ty(ty, false));
        }
        match annotation {
            TSType::TSLiteralType(literal) => {
                let value = match &literal.literal {
                    TSLiteral::StringLiteral(literal) => {
                        LiteralValue::String(literal.value.to_string())
                    }
                    TSLiteral::NumericLiteral(literal) => LiteralValue::Number(literal.value),
                    TSLiteral::BooleanLiteral(literal) => LiteralValue::Boolean(literal.value),
                    TSLiteral::UnaryExpression(unary)
                        if unary.operator == UnaryOperator::UnaryNegation =>
                    {
                        let Expression::NumericLiteral(literal) = &unary.argument else {
                            return None;
                        };
                        let negated = -literal.value;
                        LiteralValue::Number(if negated == 0.0 { 0.0 } else { negated })
                    }
                    TSLiteral::BigIntLiteral(_)
                    | TSLiteral::TemplateLiteral(_)
                    | TSLiteral::UnaryExpression(_) => return None,
                };
                Some(PlannedRecipe::literal(value))
            }
            TSType::TSParenthesizedType(parenthesized) => self.plan_declared_recipe(
                scope,
                &parenthesized.type_annotation,
                depth + 1,
                dependencies,
                selected_binders,
            ),
            TSType::TSArrayType(array) => {
                let element = self.plan_declared_recipe(
                    scope,
                    &array.element_type,
                    depth + 1,
                    dependencies,
                    selected_binders,
                )?;
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
                        selected_binders,
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
                    selected_binders,
                )?;
                Some(PlannedRecipe::readonly(operand))
            }
            TSType::TSTypeReference(reference) => self.plan_declared_reference(
                scope,
                &reference.type_name,
                reference.type_arguments.as_deref(),
                depth,
                dependencies,
                selected_binders,
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
        selected_binders: Option<&FxHashSet<TypeParamId>>,
    ) -> Option<PlannedRecipe> {
        match element {
            TSTupleElement::TSNamedTupleMember(named) if !named.optional => self
                .plan_declared_tuple_element(
                    scope,
                    &named.element_type,
                    depth,
                    dependencies,
                    selected_binders,
                ),
            TSTupleElement::TSOptionalType(_) | TSTupleElement::TSRestType(_) => None,
            _ => element.as_ts_type().and_then(|element| {
                self.plan_declared_recipe(scope, element, depth, dependencies, selected_binders)
            }),
        }
    }

    fn plan_declared_reference<'node, 'syntax>(
        &self,
        scope: ScopeId,
        name: &'node TSTypeName<'syntax>,
        arguments: Option<&'node oxc_ast::ast::TSTypeParameterInstantiation<'syntax>>,
        depth: u32,
        dependencies: &mut Vec<PlannedTypeDependency<'node>>,
        selected_binders: Option<&FxHashSet<TypeParamId>>,
    ) -> Option<PlannedRecipe> {
        let TSTypeName::IdentifierReference(identifier) = name else {
            return None;
        };
        let name = identifier.name.as_str();
        if arguments.is_none() {
            if let Some(ty) = self.lookup_type_param(name) {
                let parameter = self.interner.store().type_param(ty)?;
                return Some(PlannedRecipe::ty(
                    ty,
                    selected_binders.is_some_and(|selected| selected.contains(&parameter.id)),
                ));
            }
        }
        let group = if self.capture_compact_replay_dependencies {
            self.compact_type_decl_id_replay(scope, name)?
        } else {
            self.type_decl_id_replay(scope, name)?
        };
        if !self.type_environment.is_published() && !self.type_group_construction_is_frozen(group) {
            return None;
        }
        if self.binder.type_groups.get(group).is_none_or(|group| {
            group
                .fragments
                .iter()
                .any(|fragment| fragment.kind != TypeFragmentKind::Interface)
        }) {
            return None;
        }
        // `Array<E>` / `ReadonlyArray<E>` name the native array types, not the library
        // interface body that carries their member surface (see `library_identities`).
        if let Some(alias) = self.native_array_groups().alias_of(group) {
            let [argument] = arguments.map(|arguments| arguments.params.as_slice())? else {
                return None;
            };
            let element = self.plan_declared_recipe(
                scope,
                argument,
                depth + 1,
                dependencies,
                selected_binders,
            )?;
            let array = PlannedRecipe::array(element);
            dependencies.push(PlannedTypeDependency { scope, name });
            return Some(match alias {
                NativeArrayAlias::Array => array,
                NativeArrayAlias::ReadonlyArray => PlannedRecipe::readonly(array),
            });
        }
        let constructing_interface = if self.type_environment.is_published() {
            None
        } else {
            match self.type_environment.drafts().type_decls.get(group.index()) {
                Some(TypeDecl::Interface {
                    reserved,
                    recovery_params,
                    ..
                }) => Some((*reserved, recovery_params.clone(), false)),
                _ => None,
            }
        };
        let (template, parameters, published_template) = match constructing_interface {
            Some(interface) => interface,
            None => match self
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
        let recipe = match arguments {
            Some(arguments) if arguments.params.is_empty() && parameters.is_empty() => {
                PlannedRecipe::ty(template, false)
            }
            Some(arguments) if arguments.params.len() == parameters.len() => {
                if published_template && self.interner.store().tag(template) != TypeTag::Object {
                    return None;
                }
                let mut planned = Vec::with_capacity(arguments.params.len());
                for argument in &arguments.params {
                    planned.push(PlannedApplicationArgument {
                        recipe: self.plan_declared_recipe(
                            scope,
                            argument,
                            depth + 1,
                            dependencies,
                            selected_binders,
                        )?,
                        span: Span::from_oxc(argument.span()),
                    });
                }
                PlannedRecipe::application(template, parameters, planned)
            }
            None if parameters.is_empty() => PlannedRecipe::ty(template, false),
            _ => return None,
        };
        dependencies.push(PlannedTypeDependency { scope, name });
        Some(recipe)
    }
}

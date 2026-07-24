use super::*;
use crate::check::checker::context::TypeDecl;
use crate::types::repr::{DeclaredRecipeId, DeclaredRecipeNode};
use oxc_ast::ast::{TSTupleElement, TSTypeName};

impl<'a, 'ast, Ticket: Copy + PartialEq> Pass<'a, 'ast, Ticket> {
    pub(in crate::check::checker) fn try_plan_declared_interface_property_annotation(
        &mut self,
        scope: ScopeId,
        annotation: &TSType<'_>,
    ) -> Option<TypeId> {
        let recipe = self.plan_declared_recipe(scope, annotation)?;
        if self.declared_recipe_contains_tuple(recipe) {
            return None;
        }
        self.intern_nontrivial_declared_recipe(recipe)
    }

    pub(in crate::check::checker) fn try_plan_declared_annotation(
        &mut self,
        scope: ScopeId,
        annotation: &TSType<'_>,
    ) -> Option<TypeId> {
        let recipe = self.plan_declared_recipe(scope, annotation)?;
        self.intern_nontrivial_declared_recipe(recipe)
    }

    fn intern_nontrivial_declared_recipe(&mut self, recipe: DeclaredRecipeId) -> Option<TypeId> {
        if matches!(
            self.interner.store().declared_recipe(recipe)?.node,
            DeclaredRecipeNode::Type(_)
        ) {
            return None;
        }
        Some(self.interner.intern_declared(recipe, []))
    }

    fn declared_recipe_contains_tuple(&self, root: DeclaredRecipeId) -> bool {
        let mut pending = vec![root];
        while let Some(recipe) = pending.pop() {
            match &self
                .interner
                .store()
                .declared_recipe(recipe)
                .expect("planned recipe exists")
                .node
            {
                DeclaredRecipeNode::Tuple { .. } => return true,
                DeclaredRecipeNode::Array(element) | DeclaredRecipeNode::Readonly(element) => {
                    pending.push(*element);
                }
                DeclaredRecipeNode::Application { arguments, .. } => {
                    pending.extend(arguments.iter().copied());
                }
                DeclaredRecipeNode::Type(_) => {}
            }
        }
        false
    }

    fn plan_declared_recipe(
        &mut self,
        scope: ScopeId,
        annotation: &TSType<'_>,
    ) -> Option<DeclaredRecipeId> {
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
            return Some(
                self.interner
                    .intern_declared_recipe(DeclaredRecipeNode::Type(ty)),
            );
        }
        match annotation {
            TSType::TSParenthesizedType(parenthesized) => {
                self.plan_declared_recipe(scope, &parenthesized.type_annotation)
            }
            TSType::TSArrayType(array) => {
                let element = self.plan_declared_recipe(scope, &array.element_type)?;
                Some(
                    self.interner
                        .intern_declared_recipe(DeclaredRecipeNode::Array(element)),
                )
            }
            TSType::TSTupleType(tuple) => {
                let mut elements = Vec::with_capacity(tuple.element_types.len());
                for element in &tuple.element_types {
                    elements.push(self.plan_declared_tuple_element(scope, element)?);
                }
                Some(
                    self.interner
                        .intern_declared_recipe(DeclaredRecipeNode::Tuple {
                            elements,
                            rest: None,
                        }),
                )
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
                let operand = self.plan_declared_recipe(scope, &operator.type_annotation)?;
                Some(
                    self.interner
                        .intern_declared_recipe(DeclaredRecipeNode::Readonly(operand)),
                )
            }
            TSType::TSTypeReference(reference) => self.plan_declared_reference(
                scope,
                &reference.type_name,
                reference.type_arguments.as_deref(),
            ),
            _ => None,
        }
    }

    fn plan_declared_tuple_element(
        &mut self,
        scope: ScopeId,
        element: &TSTupleElement<'_>,
    ) -> Option<DeclaredRecipeId> {
        match element {
            TSTupleElement::TSNamedTupleMember(named) if !named.optional => {
                self.plan_declared_tuple_element(scope, &named.element_type)
            }
            TSTupleElement::TSOptionalType(_) | TSTupleElement::TSRestType(_) => None,
            _ => element
                .as_ts_type()
                .and_then(|element| self.plan_declared_recipe(scope, element)),
        }
    }

    fn plan_declared_reference(
        &mut self,
        scope: ScopeId,
        name: &TSTypeName<'_>,
        arguments: Option<&oxc_ast::ast::TSTypeParameterInstantiation<'_>>,
    ) -> Option<DeclaredRecipeId> {
        let TSTypeName::IdentifierReference(identifier) = name else {
            return None;
        };
        let name = identifier.name.as_str();
        if arguments.is_none() {
            if let Some(ty) = self.lookup_type_param(name) {
                self.interner.store().type_param(ty)?;
                return Some(
                    self.interner
                        .intern_declared_recipe(DeclaredRecipeNode::Type(ty)),
                );
            }
        }
        let group = self.type_decl_id_replay(scope, name)?;
        if !self.type_group_construction_is_frozen(group) {
            return None;
        }
        let (template, parameters) = match self.type_decls.get(group.index())? {
            TypeDecl::Interface {
                reserved, params, ..
            } => (*reserved, params.clone()),
            _ => return None,
        };
        if parameters.iter().any(|parameter| {
            self.interner
                .store()
                .type_param_constraint(*parameter)
                .is_some()
        }) {
            return None;
        }
        let arguments = match arguments {
            Some(arguments) if arguments.params.len() == parameters.len() => {
                let mut planned = Vec::with_capacity(arguments.params.len());
                for argument in &arguments.params {
                    planned.push(self.plan_declared_recipe(scope, argument)?);
                }
                planned
            }
            None if parameters.is_empty() => {
                return Some(
                    self.interner
                        .intern_declared_recipe(DeclaredRecipeNode::Type(template)),
                );
            }
            _ => return None,
        };
        Some(
            self.interner
                .intern_declared_recipe(DeclaredRecipeNode::Application {
                    template,
                    parameters,
                    arguments,
                }),
        )
    }
}
